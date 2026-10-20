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
                user_presets_file: self.user_presets_file.clone(),
                last_eval: None,
                monotonic_time: 0,
            },
            font_size: self.params.font_size.clone(),
            background_state: self.background_state.clone(),
            hosted_in_daw: false,
        };
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            state,
            Default::default(),
            move |egui_ctx, _queue, state| {
                state.hosted_in_daw = initial_hosted_in_daw.load(Ordering::Relaxed);
                state.editor.picked_path = initial_file.lock().unwrap().take();
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
                                        async_executor.execute_background(PluguzuTask::Update);
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
                                    if ui.button("+").clicked() {
                                        font_size_adjust = 1;
                                    }
                                    if state.editor.font_size > 4. && ui.button("-").clicked() {
                                        font_size_adjust = -1;
                                    }

                                    let dirty = state.editor.unsaved
                                        && (!state.hosted_in_daw // not in a DAW, or editing a file...
                                            || state.editor.picked_path.is_some());
                                    let file_menu = if dirty { "File*" } else { "File" };
                                    ui.menu_button(file_menu, |ui| {
                                        state.editor.file_menu(
                                            ui,
                                            &mut state.view,
                                            &mut state.preset.user_presets,
                                        )
                                    });
                                    ui.menu_button("Clock", |ui| {
                                        if ui.checkbox(&mut synced, "Sync").clicked() {
                                            dsp_state.synced.store(synced, Ordering::Relaxed)
                                        }
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
                                    async_executor.execute_background(task);
                                    output.response.request_focus();
                                } else if output.response.changed() {
                                    state.editor.updated = true;
                                    state.editor.unsaved = true;
                                }
                            }
                        };
                    });
                if font_size_adjust != 0 {
                    state.adjust_font_size(egui_ctx, font_size_adjust);
                }
            },
        )
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
            } else if let Some(path) = &self.picked_path
                && i.consume_key(egui::Modifiers::CTRL, egui::Key::S)
            {
                self.unsaved = false;
                self.update = Some(PluguzuTask::SaveCode(path.into()));
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
            self.update = Some(PluguzuTask::Update);
        }

        if ui.button("Open").clicked()
            && let Some(path) = file_dialog(false)
        {
            self.unsaved = false;
            self.picked_path = Some(path.display().to_string());
            self.update = Some(PluguzuTask::LoadCode(path))
        }
        if self.unsaved
            && let Some(path) = &self.picked_path
            && ui.button("Save").on_hover_text("ctrl-s").clicked()
        {
            self.unsaved = false;
            self.update = Some(PluguzuTask::SaveCode(path.into()));
        }

        if ui.button("Save-As").clicked()
            && let Some(path) = file_dialog(true)
        {
            self.unsaved = false;
            self.picked_path = Some(path.display().to_string());
            self.update = Some(PluguzuTask::SaveCode(path))
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
        egui::ComboBox::new("lang", "")
            .selected_text(format!("{:?}", selected))
            .show_ui(ui, |ui| {
                ui.selectable_value(selected.deref_mut(), UzuKind::Tidal, "tidal");
                ui.selectable_value(selected.deref_mut(), UzuKind::Mondo, "mondo");
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
    if let Some(p) = s.rfind('/') {
        &s[p + 1..]
    } else {
        s
    }
}

fn file_dialog(save: bool) -> Option<std::path::PathBuf> {
    let diag = rfd::FileDialog::new().add_filter("code", &["hs", "mondo"]);
    if save {
        diag.save_file()
    } else {
        diag.pick_file()
    }
}
