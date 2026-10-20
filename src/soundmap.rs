// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use serde::Deserialize;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

struct PluguzuJSON(Vec<(PathBuf, PathBuf)>);

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SoundMapRaw {
    Base(String),
    Sounds(Vec<String>),
    Pitches(HashMap<SmolStr, Vec<String>>),
    Pitch(HashMap<SmolStr, String>),
}

pub enum SoundFiles {
    Sounds(Vec<PathBuf>),
    Pitches(Vec<(u8, Vec<PathBuf>)>),
}

pub struct SoundMap(pub HashMap<(SmolStr, SmolStr), (Arc<PathBuf>, SoundFiles)>);

fn parse_bank(s: SmolStr) -> (SmolStr, SmolStr) {
    if let Some((b, v)) = s.split_once("_") {
        (b.into(), v.into())
    } else {
        ("".into(), s)
    }
}

fn note_to_midi(name: &str) -> Option<u8> {
    name.chars().last().and_then(|n| {
        n.to_digit(10).and_then(|octave| {
            match &name[0..name.len() - 1] {
                "C" | "c" => Some(0),
                "C#" | "Cs" | "c#" | "cs" => Some(1),
                "D" | "d" => Some(2),
                "D#" | "Ds" | "d#" | "ds" => Some(3),
                "E" | "e" => Some(4),
                "F" | "f" => Some(5),
                "F#" | "Fs" | "f#" | "fs" => Some(6),
                "G" | "g" => Some(7),
                "G#" | "Gs" | "g#" | "gs" => Some(8),
                "A" | "a" => Some(9),
                "A#" | "As" | "a#" | "as" => Some(10),
                "B" | "b" => Some(11),
                _ => None,
            }
            .map(|key| (octave * 12 + key + 12) as u8)
        })
    })
}
#[test]
fn test_note_to_midi() {
    for (k, v) in [("C2", 36), ("Ds1", 27), ("G#6", 92)] {
        assert_eq!(note_to_midi(k), Some(v))
    }
}

fn decode_paths(paths: Vec<String>) -> Vec<PathBuf> {
    let mut res = Vec::with_capacity(paths.len());
    for path in paths {
        match urlencoding::decode(&path) {
            Ok(path) => res.push(PathBuf::from(path.into_owned())),
            Err(err) => crate::pwarn!("{}: bad path encoding {}", path, err),
        }
    }
    res
}

fn make_pitch_list(mut notes: HashMap<SmolStr, Vec<String>>) -> SoundFiles {
    let mut pitches: Vec<(u8, Vec<PathBuf>)> = notes
        .drain()
        .map(|(k, v)| {
            (
                note_to_midi(&k).unwrap_or_else(|| {
                    crate::pwarn!("Unknown pitch {}", k);
                    36
                }),
                decode_paths(v),
            )
        })
        .collect();
    pitches.sort_by_key(|a| a.0);
    SoundFiles::Pitches(pitches)
}

pub fn load_soundmap(fp: PathBuf, root: Arc<PathBuf>) -> Result<SoundMap, Box<dyn Error>> {
    let mut json: HashMap<SmolStr, SoundMapRaw> =
        serde_json::from_reader(BufReader::new(File::open(&fp)?))?;
    let mut result = HashMap::new();
    for (k, v) in json.drain() {
        let v = match v {
            SoundMapRaw::Base(_fp) => None,
            SoundMapRaw::Sounds(xs) => Some(SoundFiles::Sounds(decode_paths(xs))),
            SoundMapRaw::Pitches(kv) => Some(make_pitch_list(kv)),
            SoundMapRaw::Pitch(mut kv) => Some(make_pitch_list(
                kv.drain().map(|(k, v)| (k, vec![v])).collect(),
            )),
        };
        if let Some(v) = v {
            result.insert(parse_bank(k), (root.clone(), v));
        }
    }
    Ok(SoundMap(result))
}

pub fn load_soundmaps(root: PathBuf) -> Result<SoundMap, Box<dyn Error>> {
    let mut root_file = root.clone();
    root_file.push("pluguzu.json");
    let root_json = PluguzuJSON(serde_json::from_reader(BufReader::new(File::open(
        &root_file,
    )?))?);
    let mut result = HashMap::new();
    for (name, base) in root_json.0 {
        let mut fp = root.clone();
        fp.push(name);
        let mut sm_root = root.clone();
        sm_root.push(base);
        let sm = load_soundmap(fp, Arc::new(sm_root))?;
        result.extend(sm.0);
    }
    Ok(SoundMap(result))
}
