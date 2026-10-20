// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines user config location.

use nice_plug::nice_warn;
use std::path::PathBuf;

pub struct Config(PathBuf);

impl Config {
    pub fn new() -> Config {
        if let Some(path) = dirs::config_dir().and_then(|mut p| {
            p.push("pluguzu");
            match std::fs::create_dir_all(&p) {
                Err(err) => {
                    nice_warn!("{:?}: Couldn't create config dir: {err}", p);
                    None
                }
                Ok(_) => Some(p),
            }
        }) {
            Config(path)
        } else {
            Config("/pluguzu".to_string().into())
        }
    }
    pub fn path(&self, path: &str) -> PathBuf {
        let mut p = self.0.clone();
        p.push(path);
        p
    }
}

pub const WELCOME: &str = r#"-- Welcome to Pluguzu
--
-- Press ctrl+enter to load the code in place.
--
-- Reference: https://tidalcycles.org/docs/reference/cycles
-- Source: https://codeberg.org/TristanCacqueray/pluguzu
--
-- Here is an example pattern to get started:
stack [
  -- a melody for MIDI synth
  fast "<1 [1.5 2?]>" $ n $
    arp "<up down diverge>" "<a'm9'8 e'7sus4'8>"
    |- "<12 [12 5]>/2"
  -- dough demo
, s "bd*4" # pS "bank" "crate" # cutoff "80 100 150 500"
]
"#;
