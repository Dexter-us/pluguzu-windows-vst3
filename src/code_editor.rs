// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::UzuKind;
use crate::syntax;
use egui::{
    Color32, FontId, TextFormat, style::ScrollStyle, text::LayoutJob, text_edit::TextEditOutput,
};

#[derive(Copy, Clone)]
struct FontSize(f32);
impl std::hash::Hash for FontSize {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        ((self.0 * 1000.0) as u64).hash(state)
    }
}

fn highlight(ctx: &egui::Context, fontsize: f32, lang: UzuKind, code: &str) -> LayoutJob {
    #[expect(non_local_definitions)]
    impl egui::cache::ComputerMut<(FontSize, UzuKind, &str), LayoutJob> for Highlighter {
        fn compute(&mut self, (fontsize, lang, code): (FontSize, UzuKind, &str)) -> LayoutJob {
            Self::highlight(fontsize.0, lang, code)
        }
    }

    ctx.memory_mut(|mem| {
        mem.caches
            .cache::<egui::cache::FrameCache<LayoutJob, Highlighter>>()
            .get((FontSize(fontsize), lang, code))
            .clone()
    })
}

#[derive(Default)]
struct Highlighter;
impl Highlighter {
    fn highlight(fontsize: f32, lang: UzuKind, mut code: &str) -> LayoutJob {
        let mut job = LayoutJob::default();
        let font_id = FontId::monospace(fontsize);
        let syntax: &syntax::Syntax = match lang {
            UzuKind::Mondo => &syntax::MONDO,
            UzuKind::Tidal => &syntax::TIDAL,
        };
        let scomment = TextFormat::simple(font_id.clone(), COMMENT);
        let sspecial = TextFormat::simple(font_id.clone(), SPECIAL);
        let sstrs = TextFormat::simple(font_id.clone(), STRS);
        let sfg = TextFormat::simple(font_id.clone(), FG);
        let sfunc = TextFormat::simple(font_id.clone(), FUNC);
        // Adapted from github.com/emilk/egui/crates/egui_extras/src/syntax_highlighting.rs
        while !code.is_empty() {
            if code.starts_with(syntax.comment) {
                let end = code.find('\n').unwrap_or(code.len());
                job.append(&code[..end], 0., scomment.clone());
                code = &code[end..];
            } else if code.starts_with('"') {
                let end = code[1..]
                    .find('"')
                    .map(|i| i + 2)
                    .or_else(|| code.find('\n'))
                    .unwrap_or(code.len());
                job.append(&code[..end], 0.0, sstrs.clone());
                code = &code[end..];
            } else if code.starts_with('#') || code.starts_with('$') {
                job.append(&code[..1], 0.0, sspecial.clone());
                code = &code[1..];
            } else if code.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                let end = code[1..]
                    .find(|c: char| !c.is_ascii_alphanumeric())
                    .map_or_else(|| code.len(), |i| i + 1);
                let word = &code[..end];
                let fmt = if syntax.keywords.contains(word) {
                    sfunc.clone()
                } else {
                    sfg.clone()
                };
                job.append(word, 0.0, fmt);
                code = &code[end..];
            } else if code.starts_with(|c: char| c.is_ascii_whitespace()) {
                let end = code[1..]
                    .find(|c: char| !c.is_ascii_whitespace())
                    .map_or_else(|| code.len(), |i| i + 1);
                job.append(&code[..end], 0.0, sfg.clone());
                code = &code[end..];
            } else {
                let mut it = code.char_indices();
                it.next();
                let end = it.next().map_or(code.len(), |(idx, _chr)| idx);
                job.append(&code[..end], 0.0, scomment.clone());
                code = &code[end..];
            }
        }
        job
    }
}

const BG: Color32 = Color32::from_rgb(0x2c, 0x2e, 0x34); // bg0
const COMMENT: Color32 = Color32::from_rgb(0x7f, 0x84, 0x90); // grey
const SPECIAL: Color32 = Color32::from_rgb(0xf3, 0x96, 0x60); // orange
const FUNC: Color32 = Color32::from_rgb(0x9e, 0xd0, 0x72); // green
const STRS: Color32 = Color32::from_rgb(0xe7, 0xc6, 0x64); // yellow
const FG: Color32 = Color32::from_rgb(0xe2, 0xe2, 0xe3);
const BORDER: Color32 = Color32::from_rgb(0x76, 0xcc, 0xce);

fn num_lines_layout(ctx: &egui::Context, fontsize: f32, rows: usize) -> LayoutJob {
    #[expect(non_local_definitions)]
    impl egui::cache::ComputerMut<(FontSize, usize), LayoutJob> for Highlighter {
        fn compute(&mut self, (fontsize, rows): (FontSize, usize)) -> LayoutJob {
            let mut job = LayoutJob::default();
            let font_id = FontId::monospace(fontsize.0);
            let mut lines = String::with_capacity(rows * 5);
            let width = rows.checked_ilog10().unwrap_or(0);
            for i in 1..rows + 1 {
                let n_width = i.checked_ilog10().unwrap_or(0);
                for _ in 0..(width - n_width) {
                    lines.push(' ');
                }
                lines.push_str(&i.to_string());
                lines.push('\n');
            }
            job.append(
                &lines[..lines.len() - 1],
                0.,
                TextFormat::simple(font_id.clone(), COMMENT),
            );
            job
        }
    }

    ctx.memory_mut(|mem| {
        mem.caches
            .cache::<egui::cache::FrameCache<LayoutJob, Highlighter>>()
            .get((FontSize(fontsize), rows))
            .clone()
    })
}

fn show_numlines(ui: &mut egui::Ui, fontsize: f32, rows: usize) {
    let mut layouter = |ui: &egui::Ui, _buf: &dyn egui::TextBuffer, _wrap_width: f32| {
        let layout_job = num_lines_layout(ui.ctx(), fontsize, rows);
        ui.fonts_mut(|f| f.layout_job(layout_job))
    };
    let width = (rows.checked_ilog10().unwrap_or(0)) as f32 * fontsize;
    egui::TextEdit::multiline(&mut "")
        .interactive(false)
        .desired_rows(rows)
        .desired_width(width)
        .layouter(&mut layouter)
        .show(ui);
}

pub fn show(
    ui: &mut egui::Ui,
    fontsize: f32,
    lang: UzuKind,
    code: &mut String,
    rows: usize,
) -> TextEditOutput {
    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap_width: f32| {
        let layout_job = highlight(ui.ctx(), fontsize, lang, buf.as_str());
        ui.fonts_mut(|f| f.layout_job(layout_job))
    };
    ui.spacing_mut().scroll = ScrollStyle::solid();
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                show_numlines(ui, fontsize, rows);
                egui::ScrollArea::horizontal()
                    .show(ui, |ui| {
                        egui::TextEdit::multiline(code)
                            .desired_rows(rows)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .background_color(BG)
                            .frame(
                                egui::Frame::new()
                                    .fill(BG)
                                    .stroke(egui::Stroke::new(1.0, BORDER)),
                            )
                            .layouter(&mut layouter)
                            .show(ui)
                    })
                    .inner
            })
            .inner
        })
        .inner
}
