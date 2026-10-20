// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the events produced by Haskell.

use atomic_float::AtomicF64;
use nice_plug::midi::NoteEvent;
use serde::Deserialize;
use smol_str::SmolStr;
use std::{collections::HashMap, sync::atomic::Ordering};

use crate::{PluguzuTransport, pwarn};

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy, Deserialize)]
pub struct LocEvent {
    pub col: usize,
    pub row: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[repr(C)]
pub struct MidiEvent {
    pub d0: u8,
    pub d1: u8,
    pub d2: u8,
}

impl MidiEvent {
    pub fn data(&self) -> [u8; 3] {
        [self.d0, self.d1, self.d2]
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EventSpan {
    pub start: f64,
    pub stop: f64,
}

#[derive(Debug, Clone, Deserialize, Ord, Eq, PartialOrd, PartialEq, Hash)]
pub struct SoundEvent {
    pub name: SmolStr,
    pub idx: i32,
    pub note: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EventKind {
    EventMidi(MidiEvent),
    EventSound(SoundEvent),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EventValue {
    String(SmolStr),
    Num(f32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub span: EventSpan,
    pub kind: EventKind,
    pub locs: Vec<LocEvent>,
    pub value: Option<HashMap<SmolStr, EventValue>>,
}

const NO_BANK: SmolStr = SmolStr::new_static("");
const BANK: SmolStr = SmolStr::new_static("bank");

impl Event {
    pub fn has_legato(&self) -> bool {
        self.value
            .as_ref()
            .map(|vs| vs.contains_key("legato"))
            .unwrap_or(false)
    }
    pub fn bank(&self) -> SmolStr {
        if let Some(EventValue::String(bank)) =
            self.value.as_ref().and_then(|value| value.get(&BANK))
        {
            bank.clone()
        } else {
            NO_BANK
        }
    }
    pub(crate) fn midi_trigger(&self) -> Option<MidiTrigger> {
        self.value
            .as_ref()
            .and_then(|v| v.get(&SmolStr::new_static("mt")))
            .and_then(|v| match v {
                EventValue::Num(v) => Some(MidiTrigger(*v as usize % TRIGGER_COUNT)),
                _ => None,
            })
    }
}

pub const TRIGGER_COUNT: usize = 12;

#[derive(Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Debug)]
pub struct MidiTrigger(pub usize);

pub struct MidiTriggers {
    starts: Vec<AtomicF64>,
}

impl Default for MidiTriggers {
    fn default() -> Self {
        let mut starts = Vec::with_capacity(12);
        for _i in 0..TRIGGER_COUNT {
            starts.push(AtomicF64::new(f64::NAN))
        }
        Self { starts }
    }
}
impl MidiTriggers {
    pub fn trigger(&self, note: &u8, start: Option<f64>) {
        let note = *note as usize % TRIGGER_COUNT;
        let value = start.unwrap_or(f64::NAN);
        pwarn!("MIDI Triggers! {note} {start:?}");
        self.starts[note].store(value, Ordering::Relaxed);
    }
    pub(crate) fn update_from_event(&self, transport: &PluguzuTransport, event: &NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { note, .. } => {
                // TODO: handle timing offset
                let start = transport.range.arc.start;
                self.trigger(note, Some(start));
                // auto play on first note
                if !transport.state.synced.load(Ordering::Relaxed) {
                    transport.state.playing.store(true, Ordering::Relaxed);
                }
            }
            NoteEvent::NoteOff { note, .. } => self.trigger(note, None),
            _ => {}
        };
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = (MidiTrigger, f64)> {
        self.starts
            .iter()
            .enumerate()
            .map(|(x, y)| (MidiTrigger(x), y.load(Ordering::Relaxed)))
    }
}
