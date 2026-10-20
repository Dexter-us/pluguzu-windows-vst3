use crate::haskell::UzuKind;
use crate::{BackgroundState, Pluguzu, PluguzuParams, PluguzuTask, TransportState};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use raw_window_handle_06::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use serde_json::{Value, json};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use wry::WebView;

const EDITOR_WIDTH: u32 = 960;
const EDITOR_HEIGHT: u32 = 640;
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

thread_local! {
    // WebView2 and its COM objects are bound to the plugin host's UI thread.
    static WEBVIEWS: RefCell<HashMap<u64, WebView>> = RefCell::new(HashMap::new());
}

static NEXT_WEBVIEW_ID: AtomicU64 = AtomicU64::new(1);

struct HwndParent(NonZeroIsize);

impl HasWindowHandle for HwndParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(self.0));
        // The host owns this HWND and it remains valid for the lifetime of the editor.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

pub(crate) struct ZenMasterEditor {
    params: Arc<PluguzuParams>,
    executor: AsyncExecutor<Pluguzu>,
    transport: Arc<TransportState>,
    background: Arc<BackgroundState>,
    initial_file: Arc<Mutex<Option<String>>>,
    fallback: Box<dyn Editor>,
    scale: Mutex<f64>,
    refresh: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    open: Arc<AtomicBool>,
}

impl ZenMasterEditor {
    pub(crate) fn new(
        params: Arc<PluguzuParams>,
        executor: AsyncExecutor<Pluguzu>,
        transport: Arc<TransportState>,
        background: Arc<BackgroundState>,
        initial_file: Arc<Mutex<Option<String>>>,
        fallback: Box<dyn Editor>,
    ) -> Self {
        Self {
            params,
            executor,
            transport,
            background,
            initial_file,
            fallback,
            scale: Mutex::new(1.0),
            refresh: Arc::new(AtomicBool::new(false)),
            dirty: Arc::new(AtomicBool::new(false)),
            open: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Editor for ZenMasterEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send> {
        let hwnd = match &parent {
            ParentWindowHandle::Win32Hwnd(hwnd) => NonZeroIsize::new(*hwnd as isize),
            _ => None,
        };
        let Some(hwnd) = hwnd else {
            crate::pwarn!("Zen Master WebView needs a Windows host window; opening egui fallback.");
            return self.fallback.spawn(parent, context);
        };

        let html = include_str!("../web-ui/dist/index.html");
        if html.len() >= MAX_HTML_BYTES {
            crate::pwarn!("Zen Master CodeMirror page exceeds WebView2's 2 MB inline limit.");
            return self.fallback.spawn(parent, context);
        }

        let path = self
            .initial_file
            .lock()
            .expect("Initial file mutex poisoned")
            .as_ref()
            .map(PathBuf::from);
        let code = self
            .params
            .code
            .lock()
            .expect("Program mutex poisoned")
            .clone();
        let lang = *self.params.lang.lock().expect("Language mutex poisoned");
        let language = language_name(lang);
        let directory = path
            .as_deref()
            .and_then(Path::parent)
            .or_else(|| path.as_deref())
            .map(|path| path.display().to_string())
            .or_else(|| std::env::current_dir().ok().map(|path| path.display().to_string()))
            .unwrap_or_default();
        let init_data = json!({
            "code": code,
            "language": language,
            "path": path.as_ref().map(|path| path.display().to_string()).unwrap_or_default(),
            "directory": directory,
            "synced": self.transport.synced.load(Ordering::Relaxed),
            "bpm": self.transport.bpm.load(Ordering::Relaxed),
        });
        let init_script = format!(
            "window.__ZEN_MASTER_INITIAL = {};",
            serde_json::to_string(&init_data).expect("Zen Master initial state is serializable")
        );
        let scale = *self.scale.lock().expect("Editor scale mutex poisoned");
        let bounds = wry::Rect {
            position: wry::dpi::PhysicalPosition::new(0, 0).into(),
            size: wry::dpi::PhysicalSize::new(
                (EDITOR_WIDTH as f64 * scale).round() as u32,
                (EDITOR_HEIGHT as f64 * scale).round() as u32,
            )
            .into(),
        };
        let id = NEXT_WEBVIEW_ID.fetch_add(1, Ordering::Relaxed);
        let current_path = Arc::new(Mutex::new(path));
        let params = self.params.clone();
        let executor = self.executor.clone();
        let transport = self.transport.clone();
        let background = self.background.clone();
        let refresh = self.refresh.clone();
        let dirty = self.dirty.clone();
        let context_for_ipc = context.clone();
        let parent_window = HwndParent(hwnd);

        let result = wry::WebViewBuilder::new()
            .with_bounds(bounds)
            .with_initialization_script(init_script)
            .with_html(html)
            .with_ipc_handler(move |request| {
                let Ok(message) = serde_json::from_str::<Value>(request.body()) else {
                    return;
                };
                handle_message(
                    id,
                    message,
                    &params,
                    &executor,
                    &transport,
                    &background,
                    &current_path,
                    &refresh,
                    &dirty,
                    context_for_ipc.as_ref(),
                );
            })
            .build_as_child(&parent_window);

        let webview = match result {
            Ok(webview) => webview,
            Err(error) => {
                crate::pwarn!(
                    "Could not start Zen Master WebView2 editor ({error}); opening egui fallback."
                );
                return self.fallback.spawn(parent, context);
            }
        };

        WEBVIEWS.with(|webviews| {
            webviews.borrow_mut().insert(id, webview);
        });
        self.initial_file
            .lock()
            .expect("Initial file mutex poisoned")
            .take();
        self.open.store(true, Ordering::Release);

        Box::new(WebViewEditorHandle {
            id,
            owner_thread: thread::current().id(),
            open: self.open.clone(),
        })
    }

    fn size(&self) -> (u32, u32) {
        (EDITOR_WIDTH, EDITOR_HEIGHT)
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        if !factor.is_finite() || !(0.5..=3.0).contains(&factor) {
            return false;
        }
        if self.open.load(Ordering::Acquire) {
            return false;
        }
        *self.scale.lock().expect("Editor scale mutex poisoned") = factor as f64;
        let _ = self.fallback.set_scale_factor(factor);
        true
    }

    fn param_value_changed(&self, id: &str, value: f32) {
        self.fallback.param_value_changed(id, value);
    }

    fn param_modulation_changed(&self, id: &str, offset: f32) {
        self.fallback.param_modulation_changed(id, offset);
    }

    fn param_values_changed(&self) {
        self.refresh.store(true, Ordering::Release);
        self.fallback.param_values_changed();
    }
}

struct WebViewEditorHandle {
    id: u64,
    owner_thread: ThreadId,
    open: Arc<AtomicBool>,
}

impl Drop for WebViewEditorHandle {
    fn drop(&mut self) {
        self.open.store(false, Ordering::Release);
        if thread::current().id() == self.owner_thread {
            WEBVIEWS.with(|webviews| {
                webviews.borrow_mut().remove(&self.id);
            });
        } else {
            // WebView2 must be destroyed on the thread that created it. The host normally drops
            // editor handles on its GUI thread; avoid a wrong-thread COM release if it does not.
            crate::pwarn!("Zen Master editor handle dropped away from its UI thread.");
        }
    }
}

fn handle_message(
    id: u64,
    message: Value,
    params: &PluguzuParams,
    executor: &AsyncExecutor<Pluguzu>,
    transport: &TransportState,
    background: &BackgroundState,
    current_path: &Mutex<Option<PathBuf>>,
    refresh: &AtomicBool,
    dirty: &AtomicBool,
    context: &dyn GuiContext,
) {
    match message.get("kind").and_then(Value::as_str).unwrap_or_default() {
        "edit" => {
            if let Some(code) = message.get("code").and_then(Value::as_str) {
                *params.code.lock().expect("Program mutex poisoned") = code.to_owned();
                dirty.store(true, Ordering::Release);
            }
        }
        "run" => {
            if let Some(code) = message.get("code").and_then(Value::as_str) {
                let code_changed = {
                    let mut current_code = params.code.lock().expect("Program mutex poisoned");
                    let changed = *current_code != code;
                    *current_code = code.to_owned();
                    changed
                };
                if code_changed {
                    dirty.store(true, Ordering::Release);
                }
            }
            if set_language(params, message.get("language").and_then(Value::as_str)) {
                dirty.store(true, Ordering::Release);
            }
            executor.execute_background(PluguzuTask::Update);
            if dirty.swap(false, Ordering::AcqRel) {
                notify_host_program_changed(context, params);
            }
        }
        "stop" => transport.playing.store(false, Ordering::Relaxed),
        "sync" => {
            if let Some(enabled) = message.get("enabled").and_then(Value::as_bool) {
                transport.synced.store(enabled, Ordering::Relaxed);
            }
        }
        "bpm" => {
            if let Some(bpm) = message.get("bpm").and_then(Value::as_f64)
                && bpm.is_finite()
                && (20.0..=300.0).contains(&bpm)
            {
                transport.bpm.store(bpm as f32, Ordering::Relaxed);
            }
        }
        "language" => {
            if set_language(params, message.get("language").and_then(Value::as_str)) {
                dirty.store(true, Ordering::Release);
            }
        }
        "list" => list_directory(id, &message, current_path),
        "open" => open_file(
            id,
            &message,
            params,
            executor,
            current_path,
            dirty,
            context,
        ),
        "save" => save_file(id, &message, params, current_path, dirty, context),
        "status" => {
            let error = background.error.lock().expect("Error mutex poisoned").clone();
            let sample_warning = background
                .sample_warning
                .lock()
                .expect("Sample warning mutex poisoned")
                .clone();
            let mut status = json!({
                "type": "status",
                "playing": transport.playing.load(Ordering::Relaxed),
                "synced": transport.synced.load(Ordering::Relaxed),
                "bpm": transport.bpm.load(Ordering::Relaxed),
                "error": error,
                "sampleWarning": sample_warning,
            });
            if refresh.swap(false, Ordering::AcqRel) {
                status["code"] = json!(
                    params
                        .code
                        .lock()
                        .expect("Program mutex poisoned")
                        .clone()
                );
                dirty.store(false, Ordering::Release);
            }
            send_to_webview(id, &status);
        }
        _ => {}
    }
}

fn set_language(params: &PluguzuParams, language: Option<&str>) -> bool {
    if let Some(language) = language {
        let mut current = params.lang.lock().expect("Language mutex poisoned");
        let next = if language.eq_ignore_ascii_case("mondo") {
            UzuKind::Mondo
        } else {
            UzuKind::Tidal
        };
        let changed = *current != next;
        *current = next;
        return changed;
    }
    false
}

fn language_name(language: UzuKind) -> &'static str {
    match language {
        UzuKind::Tidal => "tidal",
        UzuKind::Mondo => "mondo",
    }
}

fn notify_host_program_changed(context: &dyn GuiContext, params: &PluguzuParams) {
    let setter = ParamSetter::new(context);
    setter.begin_set_parameter(&params.version);
    setter.set_parameter(
        &params.version,
        params.version.value().overflowing_add(1).0,
    );
    setter.end_set_parameter(&params.version);
}

fn list_directory(id: u64, message: &Value, current_path: &Mutex<Option<PathBuf>>) {
    let requested = message.get("path").and_then(Value::as_str).unwrap_or_default();
    let directory = if requested.is_empty() {
        current_path
            .lock()
            .expect("Current file mutex poisoned")
            .as_deref()
            .and_then(Path::parent)
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(requested)
    };

    let directory = directory.canonicalize().unwrap_or(directory);
    if !directory.is_dir() {
        send_to_webview(
            id,
            &json!({
                "type": "directory",
                "path": directory.display().to_string(),
                "entries": [],
                "error": format!("Folder not found: {}", directory.display()),
            }),
        );
        return;
    }

    let mut entries = Vec::new();
    let mut error = None;
    match std::fs::read_dir(&directory) {
        Ok(items) => {
            for item in items {
                match item {
                    Ok(item) => {
                        let path = item.path();
                        let is_directory = path.is_dir();
                        let is_code_file = path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| {
                                extension.eq_ignore_ascii_case("hs")
                                    || extension.eq_ignore_ascii_case("mondo")
                            });
                        if is_directory || is_code_file {
                            entries.push(json!({
                                "name": item.file_name().to_string_lossy(),
                                "path": path.display().to_string(),
                                "directory": is_directory,
                            }));
                        }
                    }
                    Err(item_error) => error = Some(item_error.to_string()),
                }
            }
        }
        Err(read_error) => error = Some(read_error.to_string()),
    }
    entries.sort_by(|left, right| {
        let left_dir = left["directory"].as_bool().unwrap_or(false);
        let right_dir = right["directory"].as_bool().unwrap_or(false);
        let left_name = left["name"].as_str().unwrap_or_default().to_lowercase();
        let right_name = right["name"].as_str().unwrap_or_default().to_lowercase();
        right_dir
            .cmp(&left_dir)
            .then_with(|| left_name.cmp(&right_name))
    });
    send_to_webview(
        id,
        &json!({
            "type": "directory",
            "path": directory.display().to_string(),
            "entries": entries,
            "error": error.unwrap_or_default(),
        }),
    );
}

fn open_file(
    id: u64,
    message: &Value,
    params: &PluguzuParams,
    executor: &AsyncExecutor<Pluguzu>,
    current_path: &Mutex<Option<PathBuf>>,
    dirty: &AtomicBool,
    context: &dyn GuiContext,
) {
    let Some(path) = message.get("path").and_then(Value::as_str).map(PathBuf::from) else {
        send_file_error(id, "Select a file first.");
        return;
    };
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("hs") && !extension.eq_ignore_ascii_case("mondo") {
        send_file_error(id, "Choose a .hs or .mondo code file.");
        return;
    }
    match std::fs::read_to_string(&path) {
        Ok(code) => {
            *params.code.lock().expect("Program mutex poisoned") = code.clone();
            *params.lang.lock().expect("Language mutex poisoned") =
                if extension.eq_ignore_ascii_case("mondo") {
                    UzuKind::Mondo
                } else {
                    UzuKind::Tidal
                };
            *current_path.lock().expect("Current file mutex poisoned") = Some(path.clone());
            dirty.store(false, Ordering::Release);
            executor.execute_background(PluguzuTask::Update);
            notify_host_program_changed(context, params);
            send_to_webview(
                id,
                &json!({
                    "type": "loaded",
                    "code": code,
                    "language": language_name(*params.lang.lock().expect("Language mutex poisoned")),
                    "path": path.display().to_string(),
                }),
            );
        }
        Err(error) => send_file_error(id, &format!("Could not open file: {error}")),
    }
}

fn save_file(
    id: u64,
    message: &Value,
    params: &PluguzuParams,
    current_path: &Mutex<Option<PathBuf>>,
    dirty: &AtomicBool,
    context: &dyn GuiContext,
) {
    let requested_path = message
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if requested_path.is_empty() {
        send_file_error(id, "Enter a file name.");
        return;
    }
    let language = message
        .get("language")
        .and_then(Value::as_str)
        .map(|value| {
            if value.eq_ignore_ascii_case("mondo") {
                UzuKind::Mondo
            } else {
                UzuKind::Tidal
            }
        })
        .unwrap_or_else(|| *params.lang.lock().expect("Language mutex poisoned"));
    let language_changed = set_language(params, Some(language_name(language)));
    let path = with_code_extension(PathBuf::from(requested_path), language);
    let code = message
        .get("code")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| params.code.lock().expect("Program mutex poisoned").clone());
    let code_changed = {
        let mut current_code = params.code.lock().expect("Program mutex poisoned");
        let changed = *current_code != code;
        *current_code = code.clone();
        changed
    };
    match std::fs::write(&path, code.as_bytes()) {
        Ok(()) => {
            let had_unsaved_changes = dirty.swap(false, Ordering::AcqRel);
            if language_changed || code_changed || had_unsaved_changes {
                notify_host_program_changed(context, params);
            }
            *current_path.lock().expect("Current file mutex poisoned") = Some(path.clone());
            send_to_webview(
                id,
                &json!({
                    "type": "saved",
                    "path": path.display().to_string(),
                }),
            );
        }
        Err(error) => {
            dirty.store(true, Ordering::Release);
            send_file_error(id, &format!("Could not save file: {error}"));
        }
    }
}

fn with_code_extension(mut path: PathBuf, language: UzuKind) -> PathBuf {
    let extension = if language == UzuKind::Mondo {
        "mondo"
    } else {
        "hs"
    };
    let is_valid_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|current| {
            current.eq_ignore_ascii_case("hs") || current.eq_ignore_ascii_case("mondo")
        });
    if !is_valid_extension {
        path.set_extension(extension);
    }
    path
}

fn send_file_error(id: u64, error: &str) {
    send_to_webview(id, &json!({ "type": "file-error", "error": error }));
}

fn send_to_webview(id: u64, message: &Value) {
    let Ok(message) = serde_json::to_string(message) else {
        return;
    };
    let script = format!("window.receiveRustMessage({message});");
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&id)
            && let Err(error) = webview.evaluate_script(&script)
        {
            crate::pwarn!("Could not update Zen Master editor: {error}");
        }
    });
}
