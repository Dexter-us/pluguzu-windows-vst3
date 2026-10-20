// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use dough_sys::Source;
use std::ffi::c_int;

use crate::{
    PluguzuTransport, debug,
    events::{Event, EventValue, SoundEvent},
    pwarn,
    samples::SampleLibrary,
};

pub struct Dough {
    pub sr: f64,
    pos: usize,
    frame_count: usize,
    engine: dough_sys::Engine,
}

impl Dough {
    pub fn new() -> Self {
        Dough {
            sr: 0.,
            pos: 0,
            frame_count: 0,
            engine: dough_sys::Engine::default(),
        }
    }
    pub fn init_buffer(&mut self, frame_count: usize) {
        self.frame_count = frame_count;
        self.pos = 0;
    }
    pub fn initialize(&mut self, sr: f64) {
        if self.sr == 0. {
            unsafe { dough_sys::dough_engine_init(&mut self.engine, sr as c_int, 64, 2, 3, 0, 0) };
            unsafe { dough_sys::engine = self.engine };
            self.sr = sr;
        } else if self.sr != sr {
            todo!("Sample size change is not supported!")
        }
    }
    pub fn advance(&mut self, until: usize, samples: &mut [&mut [f32]]) {
        let max = until.min(self.frame_count);
        #[allow(clippy::needless_range_loop)]
        for idx in self.pos..max {
            let mut arr = [0., 0.];
            unsafe { dough_sys::gen_sample(&mut self.engine, arr.as_mut_ptr(), 0) };
            samples[0][idx] = arr[0];
            samples[1][idx] = arr[1];
        }
        self.pos = max;
    }
    pub fn process(&mut self, sound: &Sound) {
        unsafe { dough_sys::process_engine_event(&mut self.engine, &sound.0) };
    }
    pub fn stop(&mut self) {
        unsafe { dough_sys::panic(&mut self.engine) };
    }

    pub fn play_sound_event(
        &mut self,
        transport: &PluguzuTransport,
        samples_library: &SampleLibrary,
        ev: &Event,
        span_start: f64,
        sev: &SoundEvent,
        buffer: &mut [&mut [f32]],
    ) {
        let mut sound = crate::dough::Sound::new();
        let synth_sound = sound.default_sound(sev.name.as_str());
        if synth_sound || ev.has_legato() {
            sound.apply_param(
                "duration",
                ((ev.span.stop - ev.span.start) * transport.cycle_transport.cycle_length_secs)
                    as f32,
            );
        }
        if !synth_sound {
            match samples_library.get_sample(ev.bank(), sev) {
                Some(samples) => {
                    sound.file_source(&samples);
                }
                None => {
                    pwarn!("Sample not ready! {:?}", sev);
                    return;
                }
            }
        }
        if let Some(value) = ev.value.as_ref() {
            for (k, v) in value.iter() {
                if let EventValue::Num(val) = v {
                    if k == "legato" {
                        sound.0.duration *= *val
                    } else {
                        sound.apply_param(k, *val)
                    }
                }
            }
        }
        sound.apply_param("midinote", sev.note as f32);
        debug!(
            "{:.4}: playing sample {} at {}Hz for {} with {ev:?}",
            ev.span.start, sev.name, sound.0.freq, sound.0.duration,
        );
        let frame_offset = transport.event_offset(span_start) as usize;
        self.advance(frame_offset.saturating_sub(1), buffer);
        self.process(&sound);
    }
}

pub struct Sample {
    _samples: Vec<f32>,
    c: dough_sys::FileSource,
}

use thiserror::Error;
#[derive(Error, Debug)]
pub enum SampleError {
    #[error("couldn't read file")]
    ReadError,

    #[error("invalid sndfile")]
    SoundFile(sndfile::SndFileError),

    #[error("invalid sample rate convertion")]
    SampleRate(#[from] samplerate::Error),
}

impl Sample {
    pub fn from_file(
        path: &std::path::Path,
        freq: f32,
        sample_rate: u32,
    ) -> Result<Sample, SampleError> {
        use sndfile::*;
        let mut snd = sndfile::OpenOptions::ReadOnly(ReadOptions::Auto)
            .from_path(path)
            .map_err(SampleError::SoundFile)?;

        let samples = snd.read_all_to_vec().map_err(|_| SampleError::ReadError)?;
        let samples = samplerate::convert(
            snd.get_samplerate() as u32,
            sample_rate,
            snd.get_channels(),
            samplerate::converter_type::ConverterType::SincFastest,
            &samples,
        )
        .map_err(SampleError::SampleRate)?;
        let c = dough_sys::FileSource {
            pcm: samples.as_ptr(),
            frames: (samples.len() / snd.get_channels()) as c_int,
            channels: snd.get_channels() as c_int,
            freq,
            pos: 0.0,
        };
        Ok(Sample {
            _samples: samples,
            c,
        })
    }
}

pub struct Sound(pub(crate) dough_sys::Event);

impl Sound {
    pub fn new() -> Self {
        let mut ev = dough_sys::Event::default();
        unsafe { dough_sys::reset_event(&mut ev) };
        ev.orbit = 0;
        Sound(ev)
    }

    pub fn default_sound(&mut self, sound: &str) -> bool {
        if let Some(sound) = synth_sound(sound) {
            self.0.sound = sound;
            true
        } else {
            self.0.sound = Source::FILE_SRC;
            false
        }
    }

    pub fn file_source(&mut self, samples: &Sample) {
        self.0.file_source = samples.c;
    }

    pub fn apply_param(&mut self, name: &str, v: f32) {
        match name {
            "midinote" => self.0.freq = midi2freq(v),
            "note" => self.0.freq = midi2freq(v + 72.),
            "speed" => self.0.speed = v,

            "gain" => self.0.gain = v,
            "pan" => self.0.pan = v,
            "velocity" => self.0.velocity = v,
            "postgain" => self.0.postgain = v,
            "gate" => self.0.gate = v,
            "duration" => self.0.duration = v,
            "pw" => self.0.pw = v,

            "attack" => self.0.attack = v,
            "decay" => self.0.decay = v,
            "sustain" => self.0.sustain = v,
            "release" => self.0.release = v,

            "cutoff" => self.0.lpf = v,
            "resonance" => self.0.lpq = v,
            "lpf" => self.0.lpf = v,
            "lpq" => self.0.lpq = v,
            "lpe" => self.0.lpe = v,
            "lpa" => self.0.lpa = v,
            "lpd" => self.0.lpd = v,
            "lps" => self.0.lps = v,
            "lpr" => self.0.lpr = v,

            "hcutoff" => self.0.hpf = v,
            "hresonance" => self.0.hpq = v,
            "hpf" => self.0.hpf = v,
            "hpq" => self.0.hpq = v,
            "hpe" => self.0.hpe = v,
            "hpa" => self.0.hpa = v,
            "hpd" => self.0.hpd = v,
            "hps" => self.0.hps = v,
            "hpr" => self.0.hpr = v,

            "bandf" => self.0.bpf = v,
            "bandq" => self.0.bpq = v,
            "bpf" => self.0.bpf = v,
            "bpq" => self.0.bpq = v,
            "bpe" => self.0.bpe = v,
            "bpa" => self.0.bpa = v,
            "bpd" => self.0.bpd = v,
            "bps" => self.0.bps = v,
            "bpr" => self.0.bpr = v,

            "penv" => self.0.penv = v,
            "patt" => self.0.patt = v,
            "pdec" => self.0.pdec = v,
            "psus" => self.0.psus = v,
            "prel" => self.0.prel = v,

            "vib" => self.0.vib = v,
            "vibmod" => self.0.vibmod = v,

            "fm" => self.0.fm = v,
            "fmh" => self.0.fmh = v,
            "fme" => self.0.fme = v,
            "fma" => self.0.fma = v,
            "fmd" => self.0.fmd = v,
            "fms" => self.0.fms = v,
            "fmr" => self.0.fmr = v,

            "am" => self.0.am = v,
            "amdepth" => self.0.amdepth = v,

            "rm" => self.0.rm = v,
            "rmdepth" => self.0.rmdepth = v,

            "phaser" => self.0.phaser = v,
            "phaserdepth" => self.0.phaserdepth = v,
            "phasersweep" => self.0.phasersweep = v,
            "phasercenter" => self.0.phasercenter = v,

            "flanger" => self.0.flanger = v,
            "flangerdepth" => self.0.flangerdepth = v,
            "flangerfeedback" => self.0.flangerfeedback = v,

            "chorus" => self.0.chorus = v,
            "chorusdepth" => self.0.chorusdepth = v,
            "chorusdelay" => self.0.chorusdelay = v,

            "coarse" => self.0.coarse = v,

            "crush" => self.0.crush = v,

            "distort" => self.0.distort = v,
            "distortvol" => self.0.distortvol = v,

            "delay" => self.0.delay = v,
            "delaytime" => self.0.delaytime = v,
            "delayfeedback" => self.0.delayfeedback = v,

            "verb" => self.0.verb = v,
            "verbdecay" => self.0.verbdecay = v,
            "verbdamp" => self.0.verbdamp = v,
            "verbpredelay" => self.0.verbpredelay = v,
            "verbdiff" => self.0.verbdiff = v,

            _ => {}
        }
    }
}

pub fn midi2freq(midi: f32) -> f32 {
    2.0_f32.powf((midi - 69.0) / 12.0) * 440.0
}

pub fn synth_sound(name: &str) -> Option<Source> {
    match name {
        "tri" => Some(Source::TRI_OSC),
        "sine" => Some(Source::SINE_OSC),
        "saw" => Some(Source::SAW_OSC),
        "zaw" => Some(Source::ZAW_OSC),
        "pulse" => Some(Source::PULSE_OSC),
        "pulze" => Some(Source::PULZE_OSC),
        "white" => Some(Source::WHITE_NOISE),
        "pink" => Some(Source::PINK_NOISE),
        "brown" => Some(Source::BROWN_NOISE),
        _ => None,
    }
}

pub fn is_synth_sound(name: &str) -> bool {
    synth_sound(name).is_some()
}

unsafe impl Send for Sample {}
unsafe impl Sync for Sample {}
unsafe impl Send for Dough {}
