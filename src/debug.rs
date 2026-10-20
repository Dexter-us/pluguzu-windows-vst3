// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This modules provides convenient logging helpers

macro_rules! debug {
    ($($rest:tt)+) => {
        #[cfg(feature = "debug")]
        nice_plug::util::permit_alloc(|| {
            println!($($rest)*)
        })
    }
}

pub(crate) use debug;

macro_rules! pwarn {
    ($($rest:tt)+) => {
        nice_plug::util::permit_alloc(|| {
            nice_plug::nice_warn!($($rest)*)
        })
    }
}

pub(crate) use pwarn;

macro_rules! info {
    ($($rest:tt)+) => {
        nice_plug::nice_log!($($rest)*)
    }
}

pub(crate) use info;
