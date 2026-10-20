// Copyright © 2026 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module contains the transport logic for the tidal cycles.
// checkout the transport_debug.rs module for a standalone demo of this transport.

/// Keep track of the current arc.
#[derive(Debug, Copy, Clone)]
pub struct CycleTransport {
    previous_end: Option<f64>,
    pub cycle_length_secs: f64,
    pub frames_per_cycle: f64,
    pub sample_rate: f64,
}

impl Default for CycleTransport {
    fn default() -> Self {
        CycleTransport {
            previous_end: None,
            cycle_length_secs: 0.,
            frames_per_cycle: 0.,
            sample_rate: 48000.,
        }
    }
}

/// A cycle arc defines the current process callback run start..end position in the tidal cycles.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CycleArc {
    pub start: f64,
    pub end: f64,
}

impl CycleArc {
    pub fn modulo(&self, bar_count: i32, offset: f64) -> Self {
        let m = bar_count as f64;
        CycleArc {
            start: (self.start - offset) % m,
            end: (self.end - offset) % m,
        }
    }
}

impl std::fmt::Display for CycleArc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "arc[{:.7}..{:.7}]", self.start, self.end)
    }
}

/// A cycle range is computer for every process callback run.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CycleRange {
    // Jumped indicate that the current range is not continuous with the last run.
    pub jumped: bool,
    pub arc: CycleArc,
}

// Note [cycle jitter tolerance]
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// in most situation, the next arc.start precisely match the last arc.end,
// however the time calculation may have some variations and we must tolerate
// some (micro cycle) jitter.
//
// when changing the tempo while playing, the host transport may even go backward,
// in that case we ensure a continuous but possibly empty arc.
//
// the downside is that seeking within the TOLERANCE (while playing) might skip
// some events.
const TOLERANCE: f64 = 0.7;

impl CycleTransport {
    pub fn update_transport(
        &mut self,
        frames_count: i64,
        transport: &nice_plug::prelude::Transport,
    ) -> Option<CycleRange> {
        transport.pos_beats().and_then(|pos_beats| {
            transport.tempo.map(|tempo| {
                let sig_numerator = transport.time_sig_numerator.unwrap_or(4);
                self.update_local(
                    transport.playing,
                    frames_count,
                    pos_beats,
                    tempo,
                    sig_numerator,
                )
            })
        })
    }
    pub fn update_local(
        &mut self,
        playing: bool,
        frames_count: i64,
        pos_beats: f64,
        tempo: f64,
        sig_numerator: i32,
    ) -> CycleRange {
        // Compute the length of a cycle in seconds
        let buffer_length_secs = frames_count as f64 / self.sample_rate;
        let cycle_length_secs = (60 * sig_numerator) as f64 / tempo;
        let tempo_changed = self.cycle_length_secs != cycle_length_secs;
        self.cycle_length_secs = cycle_length_secs;

        // Compute the length of a cycle in samples (for event offset)
        let buffer_length_cycles = buffer_length_secs / cycle_length_secs;
        self.frames_per_cycle = frames_count as f64 / buffer_length_cycles;

        // Compute the desired arc for the given run
        let start = pos_beats / (sig_numerator as f64);
        let end = start + buffer_length_secs / cycle_length_secs;

        if playing {
            // When playing, check if the transport seeked to a new location
            let continuous_start = self.previous_end.and_then(|previous_end| {
                let gap = (start - previous_end).abs();
                // When tempo doesn't change, the time must be strickly moving forward
                if tempo_changed || (gap < TOLERANCE && end >= previous_end) {
                    Some(previous_end)
                } else {
                    None
                }
            });
            let arc_start = continuous_start.unwrap_or(start);
            // when fast seeking or tempo change, the end might be before start...
            // so we ensure the arc is moving forward:
            let arc_end = end.max(arc_start);
            self.previous_end = Some(arc_end);

            CycleRange {
                jumped: continuous_start.is_none(),
                arc: CycleArc {
                    start: arc_start,
                    end: arc_end,
                },
            }
        } else {
            // When not playing, set the default range:
            self.previous_end = None;
            CycleRange {
                jumped: false,
                arc: CycleArc { start, end },
            }
        }
    }
}
