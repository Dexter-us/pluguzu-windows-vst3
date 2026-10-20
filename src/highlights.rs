// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module provides helper to manage the editor pixel positions

use egui::{Galley, Pos2, Rect, Vec2};
use std::ops::Add;

pub struct EditorPixels<'a> {
    top_left: Pos2,
    galley: &'a Galley,
}

impl<'a> EditorPixels<'a> {
    pub fn new(top_left: Pos2, galley: &'a Galley) -> Self {
        EditorPixels { top_left, galley }
    }

    pub fn highlight_rect(&self, col: usize, row: usize, len: usize) -> Rect {
        if let Some(placed_row) = self.galley.rows.get(row) {
            let col_pos = placed_row.x_offset(col);
            let mut end_idx = col + len;
            if len > 0 {
                // Skip trailing white space. TODO: do this processing earlier...
                while end_idx > 0 {
                    if let Some(glyph) = placed_row.glyphs.get(end_idx - 1)
                        && glyph.chr == ' '
                    {
                        end_idx -= 1;
                    } else {
                        break;
                    }
                }
            } else {
                // Find the next space
                let max_idx = placed_row.glyphs.len();
                end_idx += 1;
                while end_idx < max_idx {
                    if let Some(glyph) = placed_row.glyphs.get(end_idx)
                        && glyph.chr == ' '
                    {
                        break;
                    }
                    end_idx += 1;
                }
            }
            let mut end_pos = placed_row.x_offset(end_idx);
            if col_pos == end_pos {
                // Special case when the error is at the end of the line, add an extra space to make a box.
                end_pos += 13.0;
            };
            Rect::from_min_size(
                self.top_left
                    .add(placed_row.pos.to_vec2().add(Vec2::new(col_pos, 0.))),
                Vec2::new(end_pos - col_pos, placed_row.height()),
            )
        } else {
            Rect::NOTHING
        }
    }
}
