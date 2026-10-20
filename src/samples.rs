// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This modules defines a library of samples

use dashmap::{DashMap, mapref::one::Ref};
use smol_str::SmolStr;
use std::io::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::events::SoundEvent;
use crate::soundmap::{SoundFiles, SoundMap};
use crate::{dough, info, pwarn};

#[derive(Eq, Hash, PartialEq)]
pub struct AbsoluteSoundEvent((SmolStr, SoundEvent));

static GL_SAMPLES: std::sync::OnceLock<DashMap<AbsoluteSoundEvent, dough::Sample>> =
    std::sync::OnceLock::new();

pub struct SampleLibrary {
    soundmap: SoundMap,
    sample_rate: AtomicU32,
    pub samples: &'static DashMap<AbsoluteSoundEvent, dough::Sample>,
}

fn closest_note(pitches: &[(u8, Vec<PathBuf>)], sev: &SoundEvent) -> (i32, i32) {
    let mut closest = i32::MAX;
    let mut prev_distance = i32::MAX;
    let mut idx = 0;
    for (pitch, paths) in pitches {
        let distance = (sev.note - *pitch as i32).abs();
        if distance < (sev.note - closest).abs() {
            closest = *pitch as i32;
            idx = if paths.is_empty() {
                pwarn!("Empty paths shall not be possible!");
                0
            } else {
                sev.idx % paths.len() as i32
            };
        }
        if distance > prev_distance {
            break;
        }
        prev_distance = distance;
    }
    (idx, closest)
}

impl AbsoluteSoundEvent {
    fn from_sev(bank: SmolStr, sev: &SoundEvent, sounds: &SoundFiles) -> Self {
        let (idx, note) = match sounds {
            SoundFiles::Sounds(paths) => {
                let idx = if paths.is_empty() {
                    pwarn!("Empty paths shall not be possible!");
                    0
                } else {
                    sev.idx % paths.len() as i32
                };
                (idx, sev.note)
            }
            SoundFiles::Pitches(xs) => closest_note(xs, sev),
        };
        AbsoluteSoundEvent((
            bank,
            SoundEvent {
                name: sev.name.clone(),
                idx,
                note,
            },
        ))
    }
}

impl SampleLibrary {
    pub fn new(soundmap: SoundMap) -> Self {
        SampleLibrary {
            soundmap,
            sample_rate: AtomicU32::new(48000),
            samples: GL_SAMPLES.get_or_init(DashMap::new),
        }
    }
    pub fn set_sample_rate(&self, sr: u32) {
        self.sample_rate.store(sr, Ordering::Relaxed);
    }

    pub fn has_sample(&self, bank: SmolStr, sev: &SoundEvent) -> bool {
        match self.soundmap.0.get(&(bank.clone(), sev.name.clone())) {
            None => false,
            Some(sfs) => {
                let asev = AbsoluteSoundEvent::from_sev(bank, sev, &sfs.1);
                self.samples.contains_key(&asev)
            }
        }
    }
    pub fn get_sample(
        &self,
        bank: SmolStr,
        sev: &SoundEvent,
    ) -> Option<Ref<'_, AbsoluteSoundEvent, dough::Sample>> {
        self.soundmap
            .0
            .get(&(bank.clone(), sev.name.clone()))
            .and_then(|sfs| {
                self.samples
                    .get(&AbsoluteSoundEvent::from_sev(bank, sev, &sfs.1))
            })
    }

    pub fn load_sample(&self, bank: SmolStr, sev: &SoundEvent) -> Result<()> {
        let (base, sfs) = self
            .soundmap
            .0
            .get(&(bank.clone(), sev.name.clone()))
            .ok_or_else(|| std::io::Error::other(format!("{} {} unknown", bank, sev.name)))?;
        let sr = self.sample_rate.load(Ordering::Relaxed);
        let asev = AbsoluteSoundEvent::from_sev(bank, sev, sfs);
        let base = base.to_path_buf();
        match sfs {
            SoundFiles::Sounds(paths) => Ok(load_path(self.samples, base, 65.41, sr, paths, asev)?),
            SoundFiles::Pitches(xs) => match xs.iter().find(|k| k.0 as i32 == asev.0.1.note) {
                Some(paths) => Ok(load_path(
                    self.samples,
                    base,
                    dough::midi2freq(paths.0.into()),
                    sr,
                    &paths.1,
                    asev,
                )?),
                None => Err(std::io::Error::other(format!(
                    "Couldn't find pitch! {}",
                    asev.0.1.note
                ))),
            },
        }
    }
}

fn load_path(
    samples: &DashMap<AbsoluteSoundEvent, dough::Sample>,
    mut path: PathBuf,
    freq: f32,
    sample_rate: u32,
    paths: &[PathBuf],
    asev: AbsoluteSoundEvent,
) -> Result<()> {
    if paths.is_empty() {
        panic!("The impossible have happened, paths should be non empty");
    };
    let file = paths
        .get(asev.0.1.idx.unsigned_abs() as usize)
        .expect("Absolute path!");
    path.push(file);
    info!("Loading {:?} {:?}", path, asev.0);
    let sample = dough::Sample::from_file(path.as_path(), freq, sample_rate)
        .map_err(|err| std::io::Error::other(format!("{}: {err:?}", path.display())))?;
    samples.insert(asev, sample);
    Ok(())
}
