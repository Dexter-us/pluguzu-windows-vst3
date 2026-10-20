// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module is the main entry point.
//! It defines the Pluguzu struct which is instantiated by nice-plug as a standalone app or a DAW plugin.

mod code_editor;
mod config;
mod debug;
mod default_presets;
mod dough;
pub mod events;
pub mod events_buffer;
pub mod haskell;
mod highlights;
mod midi;
mod presets;
mod samples;
mod soundmap;
mod syntax;
pub mod transport;
pub mod transport_debug;
mod ui;
use crossbeam::queue::ArrayQueue;
pub(crate) use debug::{debug, info, pwarn};

use atomic_float::{AtomicF32, AtomicF64};
use nice_plug::prelude::*;
use nice_plug_egui::EguiState;
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::events::EventKind;
use crate::events_buffer::{EventID, Events, EventsBuffer};
use crate::haskell::{BackgroundBuffer, HaskellState, UzuKind};
use crate::midi::FutureNoteOff;
use crate::transport::{CycleArc, CycleRange, CycleTransport};

/// Info from the engine for the UI status bar
struct TransportState {
    /// Is the transport playing?
    playing: AtomicBool,
    /// Count down of the remaining bar until the next events buffer start.
    remaining: AtomicUsize,
    /// The current cycle position
    arc_pos: AtomicF64,
    /// Are we synced to an external clock?
    synced: AtomicBool,
    /// The BPM when not synced.
    bpm: AtomicF32,
}

struct PluguzuTransport {
    cycle_transport: CycleTransport,
    range: CycleRange,
    state: Arc<TransportState>,
    /// Have we performed the stop routing.
    stopped: bool,
}

struct TransportStatus {
    running: bool,
    synced: bool,
}

impl PluguzuTransport {
    /// Return the currently playing bar
    fn is_continuous(&self, ts: &TransportStatus) -> Option<i32> {
        if !ts.synced {
            Some(self.range.arc.start.floor() as i32)
        } else {
            None
        }
    }
    fn update(
        &mut self,
        frames_count: i64,
        transport: &nice_plug::prelude::Transport,
    ) -> TransportStatus {
        let synced = self.state.synced.load(Ordering::Relaxed);

        let running = if synced {
            // when synced to an external transport
            // we compute the current cycle range based on the beat positions
            if let Some(range) = self
                .cycle_transport
                .update_transport(frames_count, transport)
            {
                // tell the UI if the external transport is playing
                self.state
                    .playing
                    .store(transport.playing, Ordering::Relaxed);
                self.range = range;
            } else {
                pwarn!("Invalid transport?!");
            }
            transport.playing
        } else {
            // otherwise, we advance the cycle range continuously
            let playing = self.state.playing.load(Ordering::Relaxed);
            let tempo = self.state.bpm.load(Ordering::Relaxed) as f64;
            let sig_numerator = 4;
            // the current position is strictly based on the last buffer end
            let pos_beats = self.range.arc.end * sig_numerator as f64;
            self.range = self.cycle_transport.update_local(
                playing,
                frames_count,
                pos_beats,
                tempo,
                sig_numerator,
            );
            if !playing {
                self.range.arc.end = 0.;
            }
            playing
        };
        // always keep track of the end range for events highlights.
        self.state
            .arc_pos
            .store(self.range.arc.end, Ordering::Relaxed);
        TransportStatus { running, synced }
    }
}

/// The params that can be saved/loaded
#[derive(Params)]
pub struct PluguzuParams {
    /// The editor state like the window size.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// The current code.
    #[persist = "code"]
    code: Arc<Mutex<String>>,

    /// The uzu lang.
    #[persist = "lang"]
    lang: Arc<Mutex<UzuKind>>,

    /// The pattern length
    #[persist = "length"]
    length: Arc<AtomicU32>,

    /// The font size.
    #[persist = "font-size"]
    font_size: Arc<AtomicU8>,

    /// Code "version" number, used to tell the host when the code changed.
    #[id = "version"]
    version: IntParam,
}

impl Default for PluguzuParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(800, 600),
            code: Arc::new(Mutex::new(crate::config::WELCOME.trim_end().into())),
            lang: Arc::new(Mutex::new(UzuKind::Tidal)),
            length: Arc::new(AtomicU32::new(4)),
            font_size: Arc::new(AtomicU8::new(20)),
            version: IntParam::new(
                "version",
                0,
                IntRange::Linear {
                    min: 0,
                    max: i32::MAX,
                },
            ),
        }
    }
}

/// This structure is shared between the background executor and the UI
/// It is not accessed in the process audio callback
struct BackgroundState {
    error: Mutex<Option<String>>,
    error_row: AtomicUsize,
    error_col: AtomicUsize,
    events: Mutex<Arc<Events>>,
    events_ready: AtomicBool,
}

pub struct Pluguzu {
    /// The params saved to the DAW.
    params: Arc<PluguzuParams>,

    /// The internal transport
    transport: PluguzuTransport,

    /// The Haskell runtime and background buffer
    haskell_state: Arc<HaskellState>,

    /// User preset
    user_presets_file: Arc<PathBuf>,

    /// Initial file when running standalone
    initial_file: Arc<Mutex<Option<String>>>,
    hosted_in_daw: Arc<AtomicBool>,

    /// The pending note-offs.
    note_offs: HashMap<(u8, u8), FutureNoteOff>,

    /// Samples
    dough: crate::dough::Dough,
    samples_library: Arc<crate::samples::SampleLibrary>,

    /// The current events
    events: Option<Arc<Events>>,
    events_buffer: EventsBuffer,
    played: Arc<ArrayQueue<EventID>>,
    next_requested: bool,

    /// The state shared between the UI and the background task executor.
    background_state: Arc<BackgroundState>,
}

impl Default for Pluguzu {
    fn default() -> Self {
        #[cfg(not(feature = "debug"))]
        nice_plug::log::set_max_level(nice_plug::log::LevelFilter::Info);

        // load the soundmap on ctor
        let config = Config::new();
        let samples_dir = config.path("samples");
        let sm = soundmap::load_soundmaps(samples_dir).unwrap_or_else(|e| {
            pwarn!("Couldn't load sound map: {:?}", e);
            soundmap::SoundMap(HashMap::new())
        });

        Self {
            params: Arc::new(PluguzuParams::default()),
            transport: PluguzuTransport {
                stopped: false,
                cycle_transport: CycleTransport::default(),
                range: CycleRange {
                    jumped: true,
                    arc: CycleArc { start: 0., end: 0. },
                },
                state: Arc::new(TransportState {
                    playing: AtomicBool::new(false),
                    remaining: AtomicUsize::new(0),
                    arc_pos: AtomicF64::new(0.),
                    synced: AtomicBool::new(true),
                    bpm: AtomicF32::new(120.),
                }),
            },
            haskell_state: Arc::new(HaskellState::default()),
            user_presets_file: Arc::new(config.path("presets.md")),
            initial_file: Arc::new(Mutex::new(None)),
            hosted_in_daw: Arc::new(AtomicBool::new(true)),
            events: None,
            events_buffer: EventsBuffer::default(),
            played: Arc::new(ArrayQueue::new(100)), // number of events per ui frame
            next_requested: false,

            dough: dough::Dough::new(),
            samples_library: Arc::new(crate::samples::SampleLibrary::new(sm)),

            note_offs: HashMap::with_capacity(255 * 16),

            background_state: Arc::new(BackgroundState {
                error: Mutex::new(None),
                error_row: AtomicUsize::new(0),
                error_col: AtomicUsize::new(0),
                events: Mutex::new(Arc::new(Events::default())),
                events_ready: AtomicBool::new(false),
            }),
        }
    }
}

pub enum PluguzuTask {
    Update,
    RenderNext(i32),
    LoadCode(PathBuf),
    SaveCode(PathBuf),
}

impl Pluguzu {
    // Setup the process callback buffer.
    fn sync_events_buffer(&mut self, buffer: &mut BackgroundBuffer) {
        // Here is the tricky part.
        // We must not de-allocate the events buffer in the audio callback.
        if let Some(stale) = buffer.stale.take() {
            // This should never happen because the background thread should always free the buffer after the sync.
            nice_plug::util::permit_alloc(move || {
                nice_warn!("Stale buffer is not free, de-allocating from audio callback...");
                std::mem::drop(stale);
            })
        }
        // Store the previous events back in the buffer so that they will be freed by the background task
        buffer.stale = self.events.take();
        self.events = Some(buffer.events.clone());
    }

    fn is_ready(&self, ts: &TransportStatus, buffer_start: i32) -> bool {
        if let Some(current_bar) = self.transport.is_continuous(ts) {
            current_bar >= buffer_start
        } else {
            true
        }
    }

    fn ensure_events_buffer(&mut self, ts: &TransportStatus) -> bool {
        // Copy the buffer from the haskell thread if needed
        let buffer_start = self.haskell_state.buffer_start.load(Ordering::Relaxed);
        if self.haskell_state.updated.load(Ordering::Relaxed) && self.is_ready(ts, buffer_start) {
            let state = self.haskell_state.clone();
            let mut buffer = nice_plug::util::permit_alloc(|| state.buffer.lock());
            self.sync_events_buffer(buffer.deref_mut());
            self.next_requested = false;
            self.haskell_state
                .current_start
                .store(buffer_start, Ordering::Relaxed);
            self.haskell_state.updated.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn send_tidal_events(
        &mut self,
        context: &mut impl ProcessContext<Self>,
        buffer: &mut [&mut [f32]],
        synced: bool,
        events: &Events,
    ) {
        for (ev, span_start) in self
            .events_buffer
            .iter(events, self.transport.range, synced)
        {
            let event = events.get(&ev).unwrap();
            let _ = self.played.push(ev);
            match &event.kind {
                EventKind::EventSound(sev) => self.dough.play_sound_event(
                    &self.transport,
                    &self.samples_library,
                    event,
                    span_start,
                    sev,
                    buffer,
                ),
                EventKind::EventMidi(mev) => self.transport.send_midi_event(
                    &event.span,
                    span_start,
                    mev,
                    &mut self.note_offs,
                    context,
                ),
            }
        }
    }
}

fn path_to_lang(path: &str) -> UzuKind {
    if path.ends_with(".mondo") {
        UzuKind::Mondo
    } else {
        UzuKind::Tidal
    }
}

impl Plugin for Pluguzu {
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        // Load file on standalone start
        if context.plugin_api() == PluginApi::Standalone {
            self.transport.state.synced.store(false, Ordering::Relaxed);
            self.hosted_in_daw.store(false, Ordering::Relaxed);
            match std::env::args().nth(1) {
                Some(fp) if !fp.starts_with("-") => {
                    let lang = path_to_lang(&fp);
                    *self.params.code.lock().unwrap() = std::fs::read_to_string(&fp)
                        .unwrap_or_else(|_| match lang {
                            UzuKind::Tidal => format!("-- {fp}\n"),
                            UzuKind::Mondo => format!("// {fp}\n"),
                        });
                    *self.params.lang.lock().unwrap() = lang;
                    *self.initial_file.lock().unwrap() = Some(fp);
                }
                _ => {}
            }
        }

        // Parse the initial code
        context.execute(PluguzuTask::Update);

        // Setup sample-rate
        let sr = buffer_config.sample_rate as u32;
        self.dough.initialize(sr.into());
        self.samples_library.set_sample_rate(sr);
        self.transport.cycle_transport.sample_rate = sr as f64;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Update transport timings
        let nice_transport = context.transport();
        let frames_count = buffer.samples();
        let ts = self.transport.update(frames_count as i64, nice_transport);

        // Pass through midi events:
        while let Some(event) = context.next_event() {
            self.events_buffer
                .midi_trigger
                .update_from_event(&self.transport, &event);
            context.send_event(event)
        }

        // When not playing...
        if !ts.running {
            if !self.transport.stopped {
                self.transport
                    .send_all_note_offs(context, &mut self.note_offs);
                self.dough.stop();
                if !ts.synced
                    && self
                        .events
                        .as_ref()
                        .map(|evs| evs.bar_start > 0)
                        .unwrap_or(true)
                {
                    // Re-render for first cycle.
                    context.execute_background(PluguzuTask::RenderNext(0))
                }
                self.transport.stopped = true;
            }
            return ProcessStatus::Normal;
            // return ProcessStatus::KeepAlive;
        }
        self.transport.stopped = false;

        // Send any previously registered note-off.
        self.transport.send_note_offs(context, &mut self.note_offs);

        // Ensure the events buffer is ready
        let new_events = self.ensure_events_buffer(&ts);
        if self.transport.range.jumped {
            self.dough.stop();
            self.transport
                .send_all_note_offs(context, &mut self.note_offs);
        }

        if let Some(events) = self.events.take() {
            if new_events {
                // Share the events with the UI. Note that this is not great to share a lock with the UI, and this
                // doesn't have to be synchronous. Perhaps arc-swap can be used instead?
                nice_plug::util::permit_alloc(|| {
                    *self.background_state.events.lock().expect("lock") = events.clone();
                });
                self.background_state
                    .events_ready
                    .store(true, Ordering::Relaxed);
            }

            if let Some(current_bar) = self.transport.is_continuous(&ts)
            && !self.next_requested
            && // wait one bar
            (current_bar - events.end_bar()).abs() <= 2
            {
                // Prepare the next buffer...
                let next_start = events.bar_start + events.bar_count - 1;
                context.execute_background(PluguzuTask::RenderNext(next_start));
                self.next_requested = true;
            }

            self.dough.init_buffer(frames_count);
            let buffer_slice = buffer.as_slice();

            // Send events.
            self.send_tidal_events(context, buffer_slice, ts.synced, &events);

            // Update the remaining value for the UI.
            self.transport
                .state
                .remaining
                .store(self.events_buffer.remaining(&events), Ordering::Relaxed);

            // Play dough
            self.dough.advance(frames_count, buffer_slice);

            // Silly borrow checker :/
            self.events = Some(events);
        }
        ProcessStatus::KeepAlive
    }

    type BackgroundTask = PluguzuTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
        let lang = self.params.lang.clone();
        let cycle_count = self.params.length.clone();
        let haskell_state = self.haskell_state.clone();
        let samples_library = self.samples_library.clone();
        let background_state = self.background_state.clone();
        let transport_state = self.transport.state.clone();
        let version_ref = AtomicU32::new(0);

        let (tx, rx) = std::sync::mpsc::channel();
        let sl = self.samples_library.clone();
        std::thread::Builder::new()
            .name("sample-loader".to_string())
            .spawn(move || {
                loop {
                    let (bank, sev): (smol_str::SmolStr, events::SoundEvent) = rx.recv().unwrap();
                    if !sl.has_sample(bank.clone(), &sev)
                        && let Err(e) = sl.load_sample(bank, &sev)
                    {
                        nice_warn!("Couldn't load sample {}: {}", sev.name, e);
                    }
                }
            })
            .expect("Sample loader thread");

        Box::new(move |task| {
            let load_missing_samples = |events: &Events| {
                let mut missing = HashSet::new();
                missing.clear();
                for ev in &events.events {
                    // Collect unknown samples
                    if let EventKind::EventSound(sev) = &ev.kind
                        && !dough::is_synth_sound(sev.name.as_str())
                        && !samples_library.has_sample(ev.bank(), sev)
                    {
                        missing.insert((ev.bank(), sev.clone()));
                    }
                }
                // Dispatch task to load missing samples
                for (bank, sev) in missing.drain() {
                    if let Err(e) = tx.send((bank, sev)) {
                        nice_warn!("Couldn't load sample: {}", e);
                    }
                }
            };

            let version = version_ref.load(Ordering::Relaxed);
            let cycle_count = cycle_count.load(Ordering::Relaxed) as i32;
            let do_render = |start, len| {
                let events = haskell_state
                    .rts
                    .lock()
                    .render(start, start + len, version + 1);
                pwarn!("Rendered at {start} for {len} into {} events", events.count);

                haskell_state.buffer_start.store(start, Ordering::Relaxed);

                // Share the events with the process callback.
                {
                    let mut bg_buf = haskell_state.buffer.lock();
                    bg_buf.update(events.clone());
                }
                haskell_state.updated.store(true, Ordering::Relaxed);

                load_missing_samples(&events);

                // Auto play on first update (skipping the initial one on load)
                if version == 1 && !transport_state.synced.load(Ordering::Relaxed) {
                    transport_state.playing.store(true, Ordering::Relaxed);
                }
                version_ref.store(version + 1, Ordering::Relaxed);
            };

            let do_update = || {
                let code = code.lock().expect("lock");
                let kind = *lang.lock().expect("lock");
                // println!("Parsing {}", code);

                // Perform code parse and maybe render
                let err = haskell_state.rts.lock().parse(kind, &code);
                let is_err = match err.as_ref() {
                    Some((err, row, col)) => {
                        pwarn!("Parsing failed: {row}:{col} - {err}");
                        background_state.error_row.store(*row, Ordering::SeqCst);
                        background_state.error_col.store(*col, Ordering::SeqCst);
                        true
                    }
                    None => false,
                };
                *background_state.error.lock().expect("lock") = err.map(|(err, _, _)| err);
                if !is_err {
                    if transport_state.synced.load(Ordering::Relaxed) {
                        // When synced, the events buffer is static, always from [0..cycle_count]
                        do_render(0, cycle_count)
                    } else {
                        do_render(
                            haskell_state.current_start.load(Ordering::Relaxed),
                            cycle_count + 1,
                        )
                    };
                }
            };

            match task {
                PluguzuTask::Update => do_update(),
                PluguzuTask::RenderNext(start) => {
                    // render an extra bar so that when the last process callback overflow then
                    // we still play the very first event.
                    do_render(start, cycle_count + 1);
                }
                PluguzuTask::SaveCode(path) => {
                    let code = code.lock().expect("lock");
                    if let Err(e) = std::fs::write(&path, code.as_bytes()) {
                        nice_warn!("{}: couldn't save {e}", path.display())
                    } else {
                        info!("{}: saved!", path.display())
                    }
                }
                PluguzuTask::LoadCode(path) => {
                    match std::fs::read_to_string(&path) {
                        Ok(new_code) => {
                            if !new_code.is_empty() {
                                *code.lock().expect("lock") = new_code;
                                *lang.lock().expect("lock") =
                                    path_to_lang(&path.display().to_string());
                                do_update()
                            }
                        }
                        Err(e) => nice_warn!("{}: couldn't load: {e}", path.display()),
                    };
                }
            }
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        self.do_editor(async_executor)
    }

    type SysExMessage = ();

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            // This is also the default and can be omitted here
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    const NAME: &'static str = "Pluguzu";
    const VENDOR: &'static str = "Midirus";
    const URL: &'static str = "https://codeberg.org/TristanCacqueray/pluguzu";
    const EMAIL: &'static str = "tristan@midirus.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
}

impl ClapPlugin for Pluguzu {
    const CLAP_ID: &'static str = "com.midirus.pluguzu";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("uzu-lang live editor");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for Pluguzu {
    const VST3_CLASS_ID: [u8; 16] = *b"PluguzuYeahBoyyy";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Generator];
}

#[cfg(not(feature = "debug-plugin"))]
nice_export_clap!(Pluguzu);
#[cfg(not(feature = "debug-plugin"))]
nice_export_vst3!(Pluguzu);
