// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines midi (and sound) event management.

use std::collections::HashMap;

use crate::{
    Pluguzu, PluguzuTransport, debug,
    events::{EventSpan, MidiEvent},
};
use nice_plug::{midi::NoteEvent, nice_error, prelude::ProcessContext};

#[derive(Debug)]
pub struct FutureNoteOff {
    when: f64,
    channel: u8,
}

impl PluguzuTransport {
    /// Calculate the event timing offset withing the current process buffer.
    pub(crate) fn event_offset(&self, when: f64) -> u32 {
        ((when - self.range.arc.start) * self.cycle_transport.frames_per_cycle) as u32
    }

    /// Send a note-off event with a pre-computed timing.
    fn do_send_note_off(
        &self,
        timing: u32,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        let mev = NoteEvent::NoteOff {
            timing,
            voice_id: None,
            channel: ev.channel,
            note,
            velocity: 0.,
        };
        debug!("{}: sending note off - {:?}", self.range.arc, mev);
        context.send_event(mev);
    }

    /// Send a note-off event.
    fn send_note_off(
        &self,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        self.do_send_note_off(self.event_offset(ev.when), note, ev, context)
    }

    pub fn send_all_note_offs(
        &self,
        context: &mut impl ProcessContext<Pluguzu>,
        note_offs: &mut HashMap<(u8, u8), FutureNoteOff>,
    ) {
        note_offs.retain(|(note, _), ev| {
            self.do_send_note_off(0, *note, ev, context);
            false
        });
    }
    pub fn send_note_offs(
        &self,
        context: &mut impl ProcessContext<Pluguzu>,
        note_offs: &mut HashMap<(u8, u8), FutureNoteOff>,
    ) {
        note_offs.retain(|(note, _), ev| {
            if ev.when >= self.range.arc.start && ev.when < self.range.arc.end {
                self.send_note_off(*note, ev, context);
                false
            } else {
                true
            }
        });
    }

    pub fn send_midi_event(
        &self,
        span: &EventSpan,
        span_start: f64,
        ev: &MidiEvent,
        note_offs: &mut HashMap<(u8, u8), FutureNoteOff>,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        let offset = if span_start > self.range.arc.start {
            self.event_offset(span_start)
        } else {
            0
        };
        match NoteEvent::<()>::from_midi(offset, &ev.data()) {
            Ok(mev) => {
                let off_when = self.range.arc.start + (span.stop - span.start);
                if let NoteEvent::NoteOn { note, channel, .. } = mev {
                    if off_when < self.range.arc.end {
                        // This is a fast event, schedule the note-off right away
                        self.send_note_off(
                            note,
                            &FutureNoteOff {
                                when: off_when,
                                channel,
                            },
                            context,
                        )
                    } else {
                        // Schedule the note-off for later
                        let prev = note_offs.insert(
                            (note, channel),
                            FutureNoteOff {
                                when: off_when,
                                channel,
                            },
                        );
                        if let Some(prev) = prev {
                            // Send any existing note-off
                            self.do_send_note_off(0, note, &prev, context)
                        }
                    }
                }
                debug!(
                    "{}: sending event {:.4} {:.4} {} {:?}",
                    self.range.arc, span.start, span_start, offset, mev
                );
                nice_plug::util::permit_alloc(|| context.send_event(mev));
            }
            Err(err) => nice_error!("Couldn't decode {:?} {:?}", ev, err),
        }
    }
}
