// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The command line entry point

use pluguzu::{
    events_buffer::EventsBuffer,
    haskell::{HaskellRuntime, UzuKind},
    transport::{CycleArc, CycleRange},
};

fn main() {
    let mut args = std::env::args().collect::<Vec<_>>();
    if let Some(cmd) = args.get(1)
        && cmd == "debug"
    {
        let (arg, kind) = match args.get(2) {
            Some(cmd) if cmd == "mondo" => (3, UzuKind::Mondo),
            _ => (2, UzuKind::Tidal),
        };
        let pat = args.get(arg).expect("a pattern");
        let mut ps = HaskellRuntime::default();
        if let Some(err) = ps.parse(kind, pat) {
            println!("Parse failure: {err:?}");
        } else {
            let events = ps.render(0, 4, 1);
            println!("Events: {:?}", &events.events);
            // for (nr, ev) in events.per_trigger[0].1.iter().enumerate() {
            //     println!("{nr}=> {ev:?}");
            // }
            // println!("Iter:");
            let mut buf = EventsBuffer::default();
            buf.midi_trigger.trigger(&1, Some(0.));
            {
                println!("Iter1:");
                let evs = buf.iter(
                    &events,
                    CycleRange {
                        jumped: true,
                        arc: CycleArc {
                            start: 0.,
                            end: 0.5,
                        },
                    },
                    true,
                );
                for ev in evs {
                    println!("=> {ev:?}")
                }
            }
            {
                println!("Iter2:");
                let evs = buf.iter(
                    &events,
                    CycleRange {
                        jumped: true,
                        arc: CycleArc {
                            start: 0.5,
                            end: 1.,
                        },
                    },
                    true,
                );
                for ev in evs {
                    println!("=> {ev:?}")
                }
            }
        }
    } else {
        if let Some(path) = args.get(1)
            && !path.starts_with('-')
        {
            // The args will be read when the plugin initialize.
            // We need to remove it from what nice standalone expect.
            args.remove(1);
        }
        nice_plug::prelude::nice_export_standalone_with_args::<pluguzu::Pluguzu, _>(args);
    }
}
