// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The pluguzu UI.

use egui::{FontId, RichText, TextStyle, Vec2, text_edit::TextEditOutput};
use nice_plug::prelude::*;
use nice_plug_egui::{create_egui_editor, resizable_window::ResizableWindow};

use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::{collections::BTreeMap, sync::atomic::AtomicU32};

use egui::{Color32, Shape, Stroke, StrokeKind};

use crate::debug;
use crate::events::LocEvent;
use crate::haskell::UzuKind;
use crate::{BackgroundState, Pluguzu, PluguzuTask};
use crate::{events_buffer::EventID, highlights};
use crate::{
    events_buffer::Events,
    presets::{Presets, PresetsTags, add_preset, load_default_presets, load_user_presets},
};

const VERT_SPACE: f32 = 3.;

fn menu_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(35, 58, 75))
        .stroke(Stroke::new(1.0, Color32::from_rgb(83, 184, 198)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 2))
}

enum UIView {
    Editor,
    Preset,
}

struct PresetState {
    cat: usize,
    selected: usize,
    presets: PresetsTags,
    user_presets: Presets,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodeDialogMode {
    Open,
    SaveAs,
}

struct CodeFileDialog {
    mode: CodeDialogMode,
    extension: String,
    directory: PathBuf,
    location: String,
    entries: Vec<PathBuf>,
    selected: Option<PathBuf>,
    file_name: String,
    overwrite: Option<PathBuf>,
    error: Option<String>,
}

impl CodeFileDialog {
    fn new(
        mode: CodeDialogMode,
        extension: String,
        directory: PathBuf,
        file_name: String,
    ) -> Self {
        let mut dialog = Self {
            mode,
            extension,
            location: directory.display().to_string(),
            directory,
            entries: Vec::new(),
            selected: None,
            file_name,
            overwrite: None,
            error: None,
        };
        dialog.reload();
        dialog
    }

    fn reload(&mut self) {
        self.entries.clear();
        self.selected = None;
        self.error = None;
        match std::fs::read_dir(&self.directory) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => {
                            let path = entry.path();
                            let is_code_file = path
                                .extension()
                                .and_then(|extension| extension.to_str())
                                .is_some_and(|extension| {
                                    extension.eq_ignore_ascii_case("hs")
                                        || extension.eq_ignore_ascii_case("mondo")
                                });
                            if path.is_dir() || is_code_file {
                                self.entries.push(path);
                            }
                        }
                        Err(error) => {
                            self.error = Some(format!("Could not list a folder entry: {error}"));
                        }
                    }
                }
                self.entries.sort_by(|left, right| {
                    let left_is_file = !left.is_dir();
                    let right_is_file = !right.is_dir();
                    left_is_file.cmp(&right_is_file).then_with(|| {
                        let left_name = left
                            .file_name()
                            .map(|name| name.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        let right_name = right
                            .file_name()
                            .map(|name| name.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        left_name.cmp(&right_name)
                    })
                });
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not open folder {}: {error}",
                    self.directory.display()
                ));
            }
        }
    }

    fn navigate(&mut self, directory: PathBuf) {
        if directory.is_dir() {
            self.directory = directory;
            self.location = self.directory.display().to_string();
            self.reload();
        } else {
            self.error = Some(format!("Folder not found: {}", directory.display()));
        }
    }
}

struct EditorState {
    cycle_count: Arc<AtomicU32>,
    desired_count: Option<u32>,
    code: Arc<Mutex<String>>,
    lang: Arc<Mutex<UzuKind>>,
    font_size: f32,

    highlights: BTreeMap<LocEvent, u64>,

    played: Arc<crossbeam::queue::ArrayQueue<EventID>>,
    events: Option<Arc<Events>>,

    focused: bool,
    updated: bool,
    unsaved: bool,

    update: Option<PluguzuTask>,
    picked_path: Option<String>,
    file_dialog: Option<CodeFileDialog>,
    user_presets_file: Arc<PathBuf>,

    last_eval: Option<u64>,
    monotonic_time: u64,
}

struct UIState {
    view: UIView,
    editor: EditorState,
    preset: PresetState,
    font_size: Arc<AtomicU8>,
    hosted_in_daw: bool,
    background_state: Arc<BackgroundState>,
}

impl Pluguzu {
    pub fn do_editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let egui_state = self.params.editor_state.clone();
        // These two initial values are not available yet, they will be used in the create_egui_editor init call back
        let initial_file = self.initial_file.clone();
        let initial_hosted_in_daw = self.hosted_in_daw.clone();

        let dsp_state = self.transport.state.clone();
        let zen_transport = dsp_state.clone();
        let fallback_initial_file = initial_file.clone();
        let fallback_hosted_in_daw = initial_hosted_in_daw.clone();
        let fallback_executor = async_executor.clone();

        let state = UIState {
            view: UIView::Editor,
            preset: PresetState {
                cat: 0,
                selected: 0,
                presets: load_default_presets(),
                user_presets: vec![],
            },
            editor: EditorState {
                cycle_count: self.params.length.clone(),
                desired_count: None,
                code: self.params.code.clone(),
                lang: self.params.lang.clone(),
                font_size: 0.,
                highlights: BTreeMap::new(),
                events: None,
                played: self.played.clone(),
                focused: false,
                updated: false,
                unsaved: false,
                update: None,
                picked_path: None,
                file_dialog: None,
                user_presets_file: self.user_presets_file.clone(),
                last_eval: None,
                monotonic_time: 0,
            },
            font_size: self.params.font_size.clone(),
            background_state: self.background_state.clone(),
            hosted_in_daw: false,
        };
        let params = self.params.clone();
        let fallback_editor = create_egui_editor(
            self.params.editor_state.clone(),
            state,
            Default::default(),
            move |egui_ctx, _queue, state| {
                state.hosted_in_daw = fallback_hosted_in_daw.load(Ordering::Relaxed);
                state.editor.picked_path = fallback_initial_file.lock().unwrap().take();
                state.update_font_size(egui_ctx, state.font_size.load(Ordering::Relaxed));
            },
            move |egui_ctx, setter, _queue, state| {
                let mut font_size_adjust = 0;
                // TODO: handle input dropped_files!
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(80.0, 60.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        // Main UI
                        match state.view {
                            UIView::Preset => {
                                if let Some(changed) = state.preset.show(ui, state.editor.font_size)
                                {
                                    if let Some((new_lang, new_code)) = changed {
                                        state.editor.set_code(new_code);
                                        *state.editor.lang.lock().expect("lock") = new_lang;
                                    fallback_executor.execute_background(PluguzuTask::Update);
                                    }
                                    state.view = UIView::Editor;
                                };
                            }
                            UIView::Editor => {
                                // Top-bar
                                let mut synced = dsp_state.synced.load(Ordering::Relaxed);
                                egui::MenuBar::new().ui(ui, |ui| {
                                    let style = ui.style_mut();
                                    style.spacing.item_spacing.x = 8.;
                                    if ui
                                        .button("A+")
                                        .on_hover_text("Increase editor text size")
                                        .clicked()
                                    {
                                        font_size_adjust = 1;
                                    }
                                    if state.editor.font_size > 4.
                                        && ui
                                            .button("A−")
                                            .on_hover_text("Decrease editor text size")
                                            .clicked()
                                    {
                                        font_size_adjust = -1;
                                    }

                                    let dirty = state.editor.unsaved
                                        && (!state.hosted_in_daw // not in a DAW, or editing a file...
                                            || state.editor.picked_path.is_some());
                                    let file_menu = if dirty { "File*" } else { "File" };
                                    menu_frame().show(ui, |ui| {
                                        ui.menu_button(
                                            RichText::new(format!("{file_menu}  ▾"))
                                                .strong()
                                                .color(Color32::WHITE),
                                            |ui| {
                                                state.editor.file_menu(
                                                    ui,
                                                    &mut state.view,
                                                    &mut state.preset.user_presets,
                                                )
                                            },
                                        )
                                        .response
                                        .on_hover_text("Open, save, or manage code files");
                                    });
                                    menu_frame().show(ui, |ui| {
                                        ui.menu_button(
                                            RichText::new("Clock  ▾")
                                                .strong()
                                                .color(Color32::WHITE),
                                            |ui| {
                                                if ui.checkbox(&mut synced, "Sync").clicked() {
                                                    dsp_state.synced.store(synced, Ordering::Relaxed)
                                                }
                                            },
                                        )
                                        .response
                                        .on_hover_text("Clock and sync settings");
                                    });
                                    state.editor.render_status(ui, synced, &dsp_state);
                                });

                                // Code editor
                                let error = state.background_state.error.lock().expect("lock");
                                let status_line = if error.is_some() { 2 } else { 0 };
                                let output = state.editor.code_editor(ui, status_line);
                                state.editor.render_highlights(
                                    ui,
                                    &state.background_state,
                                    &output,
                                    status_line > 0,
                                );

                                // Bottom-bar
                                if status_line > 0 {
                                    ui.horizontal(|ui| {
                                        if let Some(error) = error.as_deref() {
                                            ui.label("Error: ");
                                            ui.label(error);
                                        }
                                    });
                                }

                                let sample_warning = state
                                    .background_state
                                    .sample_warning
                                    .lock()
                                    .expect("lock")
                                    .clone();
                                if let Some(warning) = sample_warning {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.colored_label(
                                            egui::Color32::YELLOW,
                                            "Missing sample:",
                                        );
                                        ui.label(warning);
                                        if ui.small_button("Dismiss").clicked() {
                                            *state
                                                .background_state
                                                .sample_warning
                                                .lock()
                                                .expect("lock") = None;
                                        }
                                    });
                                }

                                if let Some(task) = state.editor.update.take() {
                                    if let PluguzuTask::Update = task {
                                        if state.editor.updated {
                                            // Tell the host that the project needs to be saved
                                            setter.begin_set_parameter(&params.version);
                                            setter.set_parameter(
                                                &params.version,
                                                params.version.value().overflowing_add(1).0,
                                            );
                                            setter.end_set_parameter(&params.version);
                                        }
                                        if let Some(count) = state.editor.desired_count.take() {
                                            state
                                                .editor
                                                .cycle_count
                                                .store(count, Ordering::Relaxed);
                                        }
                                        state.editor.updated = false;
                                    }
                                    fallback_executor.execute_background(task);
                                    output.response.request_focus();
                                } else if output.response.changed() {
                                    state.editor.updated = true;
                                    state.editor.unsaved = true;
                                }
                            }
                        };
                    });
                state.editor.show_file_dialog(egui_ctx);
                if font_size_adjust != 0 {
                    state.adjust_font_size(egui_ctx, font_size_adjust);
                }
            },
        );

        #[cfg(windows)]
        {
            Some(Box::new(crate::zen_master::ZenMasterEditor::new(
                self.params.clone(),
                async_executor,
                zen_transport,
                self.background_state.clone(),
                initial_file,
                fallback_editor,
            )))
        }

        #[cfg(not(windows))]
        {
            Some(fallback_editor)
        }
    }
}

impl UIState {
    fn adjust_font_size(&mut self, ctx: &egui::Context, val: i8) {
        let mut font_size = self.font_size.load(Ordering::Relaxed);
        if val > 0 {
            font_size = font_size.saturating_add(1);
        } else {
            font_size = font_size.saturating_sub(1);
        }
        self.update_font_size(ctx, font_size);
    }
    fn update_font_size(&mut self, ctx: &egui::Context, font_size: u8) {
        self.editor.font_size = font_size as f32;
        self.font_size.store(font_size, Ordering::Relaxed);
        ctx.all_styles_mut(|style| {
            let font = FontId::monospace(font_size as f32);
            style.text_styles.insert(TextStyle::Button, font.clone());
            style.text_styles.insert(TextStyle::Body, font);
            // Make the UI more tight
            style.spacing.item_spacing.x = 4.;
        });
    }
}

impl PresetState {
    fn show(&mut self, ui: &mut egui::Ui, font_size: f32) -> Option<Option<(UzuKind, String)>> {
        // Tag selector
        let prev_cat = self.cat;
        let cat = &mut self.cat;
        ui.horizontal(|ui| {
            if !self.user_presets.is_empty() {
                ui.selectable_value(cat, usize::MAX, "User");
            }
            for (pos, (tag, _)) in self.presets.iter().enumerate() {
                ui.selectable_value(cat, pos, tag);
            }
        });
        if prev_cat != *cat {
            self.selected = 0;
        }

        ui.separator();
        // Preset selector
        let sel = &mut self.selected;
        let mut result = None;
        let presets = if *cat == usize::MAX {
            &self.user_presets
        } else if let Some((_, presets)) = self.presets.get(*cat) {
            presets
        } else {
            &vec![]
        };
        let line_height = font_size + VERT_SPACE;
        let avail_height = ui.available_height() - line_height * 2.0 + 10.;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .max_height(avail_height)
            .show(ui, |ui| {
                egui::Grid::new("my_grid")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (pos, preset) in presets.iter().enumerate() {
                            ui.selectable_value(sel, pos, &preset.name);
                            ui.label(format!("{}", preset.code.len()));
                            ui.end_row();
                        }
                    });
                if let Some(preset) = presets.get(*sel)
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    result = Some(Some((preset.lang, preset.code.to_string())));
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    result = Some(None);
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    *sel = sel.saturating_sub(1)
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    *sel = sel.saturating_add(1)
                }
            });
        ui.horizontal(|ui| {
            if let Some(preset) = presets.get(*sel)
                && ui.button("Load").clicked()
            {
                result = Some(Some((preset.lang, preset.code.to_string())))
            };
            if ui.button("Cancel").clicked() {
                result = Some(None)
            };
        });

        result
    }
}

impl EditorState {
    fn set_code(&self, code: String) {
        *self.code.lock().expect("lock") = code;
    }
    fn code_editor(&mut self, ui: &mut egui::Ui, status_line: i16) -> TextEditOutput {
        if status_line > 0 {
            ui.style_mut().visuals.selection.stroke.color = Color32::from_rgb(0xfc, 0x5d, 0x7c);
        }
        let line_height = self.font_size + VERT_SPACE;
        let avail_lines = (ui.available_height() / line_height).floor() - status_line as f32;

        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter) {
                self.last_eval = Some(self.monotonic_time);
                self.update = Some(PluguzuTask::Update)
            } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
                self.save_file(false);
            }
        });
        let mut code_data = self.code.lock().expect("lock");
        let output = crate::code_editor::show(
            ui,
            self.font_size,
            *self.lang.lock().expect("lock"),
            code_data.deref_mut(),
            avail_lines as usize,
        );
        if !self.focused {
            output.response.request_focus();
            self.focused = true;
        }
        output
    }
    fn file_menu(&mut self, ui: &mut egui::Ui, view: &mut UIView, up: &mut Presets) {
        if ui.button("New").clicked() {
            self.set_code("".to_string());
            self.picked_path = None;
            self.unsaved = true;
            self.update = Some(PluguzuTask::Update);
        }

        if ui.button("Open File...").clicked() {
            self.open_file_dialog(CodeDialogMode::Open);
        }
        if ui
            .button("Save File")
            .on_hover_text("Ctrl+S")
            .clicked()
        {
            self.save_file(false);
        }

        if ui.button("Save File As...").clicked() {
            self.save_file(true);
        }
        if ui.button("Save as preset").clicked() {
            self.unsaved = false;
            self.picked_path = None;
            let code = self.code.lock().unwrap();
            let lang = self.lang.lock().unwrap();
            let new_preset = add_preset(&self.user_presets_file, *lang, &code);
            up.push(new_preset);
        }
        if ui.button("Load Preset").clicked() {
            *up = load_user_presets(&self.user_presets_file);
            *view = UIView::Preset
        }
    }

    fn save_file(&mut self, save_as: bool) {
        if save_as || self.picked_path.is_none() {
            self.open_file_dialog(CodeDialogMode::SaveAs);
            return;
        }

        if let Some(path) = self.picked_path.as_deref().map(PathBuf::from) {
            self.unsaved = false;
            self.update = Some(PluguzuTask::SaveCode(path));
        }
    }

    fn open_file_dialog(&mut self, mode: CodeDialogMode) {
        let current_dir = self
            .picked_path
            .as_deref()
            .and_then(|path| {
                std::path::Path::new(path)
                    .parent()
                    .map(PathBuf::from)
            })
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let lang = *self.lang.lock().expect("lock");
        let extension = match lang {
            UzuKind::Tidal => "hs",
            UzuKind::Mondo => "mondo",
        };
        let file_name = if mode == CodeDialogMode::SaveAs {
            self.picked_path
                .as_deref()
                .and_then(|path| {
                    std::path::Path::new(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| format!("untitled.{extension}"))
        } else {
            String::new()
        };
        self.file_dialog = Some(CodeFileDialog::new(
            mode,
            extension.to_string(),
            current_dir,
            file_name,
        ));
    }

    fn show_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.file_dialog.take() else {
            return;
        };
        let title = match dialog.mode {
            CodeDialogMode::Open => "Open code file",
            CodeDialogMode::SaveAs => "Save code file as",
        };
        let mut open = true;
        let mut cancel = false;
        let mut chosen_path = None;

        egui::Window::new(title)
            .id(egui::Id::new("pluguzu-code-file-dialog"))
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(560.0, 420.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Tidal (.hs) and Mondo (.mondo) code files");
                ui.horizontal(|ui| {
                    if ui.button("Up").clicked() {
                        let parent = dialog.directory.parent().map(PathBuf::from);
                        if let Some(parent) = parent {
                            dialog.navigate(parent);
                        }
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut dialog.location)
                            .desired_width(380.0),
                    );
                    if ui.button("Go").clicked() {
                        dialog.navigate(PathBuf::from(dialog.location.clone()));
                    }
                });
                ui.separator();

                let entries = dialog.entries.clone();
                let mut navigate_to = None;
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .show(ui, |ui| {
                        for path in entries {
                            let is_directory = path.is_dir();
                            let name = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            let label = if is_directory {
                                format!("[Folder] {name}")
                            } else {
                                name.clone()
                            };
                            let response = ui.selectable_label(
                                dialog.selected.as_ref() == Some(&path),
                                label,
                            );
                            if response.clicked() {
                                dialog.selected = Some(path.clone());
                                if dialog.mode == CodeDialogMode::SaveAs && !is_directory {
                                    dialog.file_name = name.clone();
                                }
                            }
                            if response.double_clicked() {
                                if is_directory {
                                    navigate_to = Some(path);
                                } else if dialog.mode == CodeDialogMode::Open {
                                    chosen_path = Some(path);
                                } else {
                                    dialog.file_name = name;
                                }
                            }
                        }
                    });
                if let Some(directory) = navigate_to {
                    dialog.navigate(directory);
                }

                if let Some(error) = dialog.error.as_deref() {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }

                if dialog.mode == CodeDialogMode::SaveAs {
                    ui.horizontal(|ui| {
                        ui.label("File name:");
                        ui.text_edit_singleline(&mut dialog.file_name);
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    match dialog.mode {
                        CodeDialogMode::Open => {
                            let can_open = dialog
                                .selected
                                .as_ref()
                                .is_some_and(|path| path.is_file());
                            if ui
                                .add_enabled(can_open, egui::Button::new("Open"))
                                .clicked()
                            {
                                chosen_path = dialog.selected.clone();
                            }
                        }
                        CodeDialogMode::SaveAs => {
                            if let Some(path) = dialog.overwrite.clone() {
                                ui.label("That file already exists.");
                                if ui.button("Replace").clicked() {
                                    chosen_path = Some(path);
                                    dialog.overwrite = None;
                                }
                                if ui.button("Back").clicked() {
                                    dialog.overwrite = None;
                                }
                            } else {
                                let can_save = !dialog.file_name.trim().is_empty();
                                if ui
                                    .add_enabled(can_save, egui::Button::new("Save"))
                                    .clicked()
                                {
                                    let mut path = PathBuf::from(dialog.file_name.trim());
                                    if !path.is_absolute() {
                                        path = dialog.directory.join(path);
                                    }
                                    let has_expected_extension = path
                                        .extension()
                                        .and_then(|extension| extension.to_str())
                                        .is_some_and(|extension| {
                                            extension.eq_ignore_ascii_case(&dialog.extension)
                                        });
                                    if !has_expected_extension {
                                        path.set_extension(&dialog.extension);
                                    }
                                    if path.is_dir() {
                                        dialog.error =
                                            Some("Choose a file name, not a folder.".to_string());
                                    } else if path
                                        .parent()
                                        .is_some_and(|parent| !parent.is_dir())
                                    {
                                        let parent =
                                            path.parent().unwrap_or(dialog.directory.as_path());
                                        dialog.error = Some(format!(
                                            "Folder not found: {}",
                                            parent.display()
                                        ));
                                    } else if path.exists() {
                                        dialog.overwrite = Some(path);
                                    } else {
                                        chosen_path = Some(path);
                                    }
                                }
                            }
                        }
                    }
                });
            });

        if !open || cancel {
            self.file_dialog = None;
            return;
        }

        if let Some(mut path) = chosen_path {
            match dialog.mode {
                CodeDialogMode::Open => {
                    self.unsaved = false;
                    self.picked_path = Some(path.display().to_string());
                    self.update = Some(PluguzuTask::LoadCode(path));
                }
                CodeDialogMode::SaveAs => {
                    if path.extension().is_none() {
                        let extension = match *self.lang.lock().expect("lock") {
                            UzuKind::Tidal => "hs",
                            UzuKind::Mondo => "mondo",
                        };
                        path.set_extension(extension);
                    }
                    self.unsaved = false;
                    self.picked_path = Some(path.display().to_string());
                    self.update = Some(PluguzuTask::SaveCode(path));
                }
            }
        } else {
            self.file_dialog = Some(dialog);
        }
    }

    fn render_status(
        &mut self,
        ui: &mut egui::Ui,
        synced: bool,
        dsp_state: &crate::TransportState,
    ) {
        let playing = dsp_state.playing.load(Ordering::Relaxed);
        if synced {
            let status = if playing { ">>=" } else { "|-|" };
            ui.label(status);
        } else {
            let status = if playing { "STOP" } else { "PLAY" };
            if ui.button(status).clicked() {
                dsp_state.playing.store(!playing, Ordering::Relaxed);
            }
        }

        let mut cycle_count = self
            .desired_count
            .unwrap_or_else(|| self.cycle_count.load(Ordering::Relaxed));
        if ui
            .add(egui::DragValue::new(&mut cycle_count).speed(0.1))
            .on_hover_text("The number of cycles to play")
            .changed()
            && cycle_count > 0
        {
            self.updated = true;
            self.desired_count = Some(cycle_count)
        }

        if self.updated && ui.button("UPDATE").clicked() {
            self.update = Some(PluguzuTask::Update);
        }

        if !synced {
            let mut bpm = dsp_state.bpm.load(Ordering::Relaxed);
            if ui.add(egui::DragValue::new(&mut bpm).speed(0.1)).changed() {
                dsp_state.bpm.store(bpm, Ordering::Relaxed);
            }
        }
        let arc_pos = dsp_state.arc_pos.load(Ordering::Relaxed);
        let remaining = dsp_state.remaining.load(Ordering::Relaxed);
        ui.label(RichText::new(format!(" {arc_pos:.2} | {remaining} events")));

        // Lang selector
        let mut selected = self.lang.lock().expect("lock");
        let before = *selected;
        menu_frame().show(ui, |ui| {
            egui::ComboBox::new("lang", "")
                .selected_text(format!("{:?}", selected))
                .show_ui(ui, |ui| {
                    ui.selectable_value(selected.deref_mut(), UzuKind::Tidal, "tidal");
                    ui.selectable_value(selected.deref_mut(), UzuKind::Mondo, "mondo");
                });
            });

        if *selected != before {
            self.updated = true;
            // Handle selection change
        }
        if let Some(path) = self.picked_path.as_ref() {
            ui.label(file_name(path));
        }
    }
    fn render_highlights(
        &mut self,
        ui: &mut egui::Ui,
        background_state: &BackgroundState,
        output: &TextEditOutput,
        with_error: bool,
    ) {
        if background_state
            .events_ready
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| Some(false))
            .unwrap()
        {
            debug!("Got new UI events...");
            self.events = Some(background_state.events.lock().expect("lock").clone());
        }
        let painter = ui.painter_at(output.response.rect);
        let ep = highlights::EditorPixels::new(output.galley_pos, &output.galley);

        // Remove old highlights
        let hl_age = 60;
        self.highlights
            .retain(|_, when| self.monotonic_time - *when < hl_age);

        // Add new highlights
        while let Some(ev) = self.played.pop() {
            if let Some(event) = self.events.as_ref().and_then(|evs| evs.get(&ev)) {
                for loc in &event.locs {
                    self.highlights.insert(*loc, self.monotonic_time);
                }
            }
        }

        // Draw error
        if with_error {
            let row = background_state.error_row.load(Ordering::SeqCst);
            let col = background_state.error_col.load(Ordering::SeqCst);
            if row > 0 && col > 0 {
                let shape = Shape::rect_stroke(
                    ep.highlight_rect(col - 1, row - 1, 0),
                    5.,
                    Stroke {
                        width: 2.,
                        color: Color32::from_rgb(0xff, 0x30, 0x30),
                    },
                    StrokeKind::Middle,
                );
                painter.add(shape);
            }
        }

        let shapes = self.highlights.iter().map(|(el, when)| {
            let alpha = alpha_lerp(self.monotonic_time, *when, hl_age);
            Shape::rect_stroke(
                ep.highlight_rect(el.col, el.row, el.len),
                5.,
                Stroke {
                    width: 2.,
                    color: Color32::from_rgba_unmultiplied(0xff, 0x6e, 0xc7, alpha),
                },
                StrokeKind::Middle,
            )
        });
        painter.extend(shapes);
        if let Some(last_eval) = self.last_eval
            && self.monotonic_time.saturating_sub(last_eval) < 500
        {
            let alpha = alpha_lerp(self.monotonic_time, last_eval, hl_age / 2);
            painter.add(Shape::rect_stroke(
                output.response.rect.expand(-0.5),
                5.,
                Stroke {
                    width: 9.,
                    color: Color32::from_rgba_unmultiplied(0x76, 0xcc, 0xce, alpha),
                },
                StrokeKind::Middle,
            ));
        } else {
            self.last_eval.take();
        }
        let (new_time, overflow) = self.monotonic_time.overflowing_add(1);
        if overflow {
            self.highlights.clear();
        };
        self.monotonic_time = new_time;
    }
}

fn alpha_lerp(now: u64, when: u64, age: u64) -> u8 {
    (egui::lerp(1.0..=0.3, (now - when) as f32 / age as f32) * 255.) as u8
}

fn file_name(s: &str) -> &str {
    std::path::Path::new(s)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(s)
}
