// Copyright © 2026 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::sync::Arc;

use crate::{
    events::{Event, MidiTriggers, TRIGGER_COUNT},
    pwarn,
    transport::{CycleArc, CycleRange},
};

#[derive(Default, Copy, Debug, Clone)]
pub struct EventID(usize);

#[derive(Default, Clone)]
pub struct Events {
    pub events: Vec<Event>,
    pub per_trigger: [Vec<EventID>; TRIGGER_COUNT + 1],
    pub count: usize,
    pub bar_start: i32,
    pub bar_count: i32,
    pub version: u32,
}

impl Events {
    pub fn get(&self, ev: &EventID) -> Option<&Event> {
        self.events.get(ev.0)
    }
    pub fn end_bar(&self) -> i32 {
        self.bar_start + self.bar_count
    }
    pub fn iter(&self, trigger: usize) -> impl Iterator<Item = &Event> {
        self.per_trigger[trigger]
            .iter()
            .map(|ev| &self.events[ev.0])
    }
    pub fn ev_start(&self, ev: &EventID) -> Option<f64> {
        self.get(ev).map(|ev| ev.span.start)
    }
    pub fn from_events(events: Vec<Event>, start: i32, end: i32, version: u32) -> Self {
        let mut per_trigger: [Vec<EventID>; TRIGGER_COUNT + 1] = Default::default();
        let count = events.len();
        for (idx, event) in events.iter().enumerate() {
            let ev = EventID(idx);
            match event.midi_trigger() {
                Some(mt) => per_trigger[mt.0 + 1].push(ev),
                // no mt, trigger is 0
                _ => per_trigger[0].push(ev),
            };
        }
        Self {
            events,
            per_trigger,
            count,
            version,
            bar_start: start,
            bar_count: end - start,
        }
    }
}

#[derive(Clone, Debug)]
struct ActiveTrigger {
    idx: usize,
    last_pos: usize,
    arc: CycleArc,
}

pub struct EventsBuffer {
    pub version: u32,
    pub pos: [usize; TRIGGER_COUNT + 1],
    pub trigger_start: [f64; TRIGGER_COUNT],
    pub need_reset: [bool; TRIGGER_COUNT],
    actives: Vec<ActiveTrigger>,
    pub midi_trigger: Arc<MidiTriggers>,
}

impl Default for EventsBuffer {
    fn default() -> Self {
        Self {
            version: 0,
            pos: [0; TRIGGER_COUNT + 1],
            trigger_start: [f64::NAN; TRIGGER_COUNT],
            need_reset: [false; TRIGGER_COUNT],
            midi_trigger: Arc::new(MidiTriggers::default()),
            actives: Vec::with_capacity(TRIGGER_COUNT + 1),
        }
    }
}

impl EventsBuffer {
    pub fn remaining(&self, events: &Events) -> usize {
        events.per_trigger[0].len().saturating_sub(self.pos[0])
    }
    pub fn update_actives(&mut self, events: &Events, arc: CycleArc, synced: bool) {
        self.actives.clear();
        if !events.per_trigger[0].is_empty() {
            self.actives.push(ActiveTrigger {
                idx: 0,
                last_pos: self.pos[0],
                arc: if synced {
                    // When synced, the arc wrap around the static buffer
                    arc.modulo(events.bar_count, 0.)
                } else {
                    // Otherwise the arc is continuously moving forward
                    arc
                },
            })
        }
        for (mt, start) in self.midi_trigger.iter() {
            if !start.is_nan() {
                let idx = mt.0 + 1;
                if !events.per_trigger[idx].is_empty() {
                    if self.trigger_start[mt.0] != start {
                        pwarn!("Trigger reset! {mt:?} {start}");
                        self.pos[idx] = 0;
                    }
                    self.trigger_start[mt.0] = start;

                    self.actives.push(ActiveTrigger {
                        idx,
                        last_pos: self.pos[idx],
                        arc: if synced {
                            // When synced, the arc wrap around the static buffer
                            arc.modulo(events.bar_count, start)
                        } else {
                            // Here is the tricky part... The events buffer is continuous,
                            // but the triggered pattern should always start at the first event
                            // within the region. This is using the fixed buffer length to determine
                            // where in the buffer the trigger is presently playing.
                            let base = events.bar_start as f64;
                            let count = (events.bar_count - 1) as f64;
                            let start = base + (arc.start - start) % count;
                            let end = start + (arc.end - arc.start);

                            if self.need_reset[mt.0] {
                                // When the trigger starts mid way through a buffer, we might have to
                                // catchup to the current event position after it reach the end of the buffer...
                                pwarn!("{arc} Reseting trigger {} at {start}", mt.0);
                                self.pos[idx] = events.per_trigger[idx]
                                    .iter()
                                    .position(|ev| events.ev_start(ev).unwrap_or(f64::MIN) >= start)
                                    .unwrap_or(usize::MAX);
                                self.need_reset[mt.0] = false;
                            }
                            if end > base + count {
                                // This frame overlaps with the next buffer,
                                // so next time we'll have to reset the trigger position
                                self.need_reset[mt.0] = true;
                            }
                            CycleArc { start, end }
                        },
                    });
                } else {
                    // pwarn!("Unknown trigger: {idx}")
                }
            }
        }
    }

    // when synced, the events buffer is static, from 0 to cycle count.
    pub fn iter<'a>(
        &'a mut self,
        events: &'a Events,
        range: CycleRange,
        synced: bool,
    ) -> impl Iterator<Item = (EventID, f64)> {
        self.update_actives(events, range.arc, synced);
        if events.version != self.version || range.jumped {
            self.pos.fill(0);
            for active in self.actives.iter_mut() {
                self.pos[active.idx] = events.per_trigger[active.idx]
                    .iter()
                    .position(|ev| events.ev_start(ev).unwrap_or(f64::MIN) >= active.arc.start)
                    .unwrap_or(usize::MAX);
                active.last_pos = self.pos[active.idx];
            }
            self.version = events.version;
        }

        let queue = smallvec::SmallVec::<[(EventID, f64); TRIGGER_COUNT + 1]>::new();
        // pwarn!("HERE: {} {} {:?}", range.arc, self.pos[0], self.actives);
        EventsIter {
            all_events: events,
            pos: &mut self.pos,
            actives: &mut self.actives,
            queue,
            continuous: !synced,
        }
    }
}

pub struct EventsIter<'a> {
    all_events: &'a Events,
    pos: &'a mut [usize],
    actives: &'a mut [ActiveTrigger],
    queue: smallvec::SmallVec<[(EventID, f64); TRIGGER_COUNT + 1]>,
    continuous: bool,
}

impl Iterator for EventsIter<'_> {
    type Item = (EventID, f64);
    fn next(&mut self) -> Option<(EventID, f64)> {
        let all_events = &self.all_events.events;
        for active in self.actives.iter() {
            let pos = &mut self.pos[active.idx];
            let events = &self.all_events.per_trigger[active.idx];
            let ev = if self.continuous {
                next_continuous_event(&active.arc, pos, events, all_events)
            } else {
                next_event(&active.arc, pos, active.last_pos, events, all_events)
            };
            if let Some(ev) = ev {
                self.queue.push(ev);
            }
        }
        // ensure the events are yielded in order
        self.queue.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.queue.pop()
    }
}

fn next_continuous_event(
    arc: &CycleArc,
    pos: &mut usize,
    events: &[EventID],
    all_events: &[Event],
) -> Option<(EventID, f64)> {
    events.get(*pos).and_then(|ev| {
        let event = &all_events[ev.0];
        if event.span.start >= arc.start && event.span.start < arc.end {
            *pos += 1;
            Some((*ev, event.span.start))
        } else {
            None
        }
    })
}

fn next_event(
    arc: &CycleArc,
    pos: &mut usize,
    last_pos: usize,
    events: &[EventID],
    all_events: &[Event],
) -> Option<(EventID, f64)> {
    let start = arc.start;
    let end = arc.end;
    if *pos == usize::MAX {
        // waiting for the cycles to restart...
        if start >= end {
            *pos = 0
        } else {
            return None;
        }
    }
    if events.is_empty() {
        return None;
    }
    let evid = events[*pos];
    let ev = &all_events[evid.0];

    let span_start = if start < end {
        // Non overlapping range
        if ev.span.start >= end {
            // This event starts in the future.
            return None;
        } else if ev.span.start >= start {
            // This event is due for this buffer.
            start + (ev.span.start - start)
        } else {
            // #[rustfmt::skip]
            // crate::pwarn!("{} Missed an event? {:.7}..{:.7} for ev.start={:.7}", pos, start, end, ev.span.start);
            return None;
        }
    } else {
        // Overlapping range
        if ev.span.start >= start || ev.span.start < end {
            end - ev.span.start
        } else {
            return None;
        }
    };

    *pos += 1;
    if last_pos == *pos {
        // looped around
        return None;
    }
    if *pos >= events.len() {
        *pos = usize::MAX;
    }

    Some((evid, span_start))
}
