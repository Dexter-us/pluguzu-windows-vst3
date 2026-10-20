// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The code syntax definition.

use std::collections::BTreeSet;

pub struct Syntax {
    pub comment: &'static str,
    pub keywords: BTreeSet<&'static str>,
}

lazy_static::lazy_static! {
    pub static ref TIDAL: Syntax = tidal();
    pub static ref MONDO: Syntax = mondo();
}

fn tidal() -> Syntax {
    Syntax {
        comment: "--",
        #[rustfmt::skip]
        keywords: BTreeSet::from([ // Update with grep -o "\"[a-zA-Z0-9]\+\"" /srv/codeberg.org/uzu/tidal/tidal-parse/src/Sound/Tidal/Parse.hs | tr '\n' ',' | sed 's/,/, /g'
            "True", "False", "literal", "major", "aug", "six", "sixNine", "major7", "major9", "add9", "major11",
            "add11", "major13", "add13", "dom7", "dom9", "dom11", "dom13", "sevenFlat5", "sevenSharp5", "sevenFlat9",
            "nine", "eleven", "thirteen", "minor", "diminished", "minorSharp5", "minor6", "minorSixNine", "minor7flat5",
            "minor7", "minor7sharp5", "minor7flat9", "minor7sharp9", "diminished7", "minor9", "minor11", "minor13",
            "one", "five", "sus2", "sus4", "sevenSus2", "sevenSus4", "nineSus4", "sevenFlat10", "nineSharp5",
            "minor9sharp5", "sevenSharp5flat9", "minor7sharp5flat9", "elevenSharp", "minor11sharp", "ur", "irand",
            "rand", "perlin", "silence", "silence", "sine", "saw", "isaw", "tri", "square", "cosine", "envEq", "envEqR",
            "envL", "envLR", "in", "layer", "spreadf", "ghost", "silent", "perlin2", "id", "brak", "rev", "palindrome",
            "stretch", "loopFirst", "degrade", "arpeggiate", "trigger", "rolled", "run", "listToPat", "choose",
            "cycleChoose", "stack", "fastcat", "fastCat", "slowcat", "slowCat", "cat", "randcat", "wrandcat", "s",
            "sound", "vowel", "cc", "nrpn", "cut", "nrpnn", "nrpnv", "ascii", "binary", "up", "n", "note", "midinote",
            "speed", "pan", "shape", "gain", "overgain", "overshape", "accelerate", "bandf", "bandq", "begin", "crush",
            "legato", "cutoff", "delayfeedback", "delaytime", "delay", "end", "hcutoff", "hresonance", "resonance",
            "loop", "coarse", "nudge", "amp", "velocity", "midibend", "midichan", "miditouch", "ccn", "ccv", "sseqs",
            "perlinWith", "inv", "wchoose", "timeCat", "timecat", "cF0", "perlin2With", "interlace", "overlay",
            "append", "slowAppend", "slowappend", "fastAppend", "fastappend", "const", "rotL", "rotR", "binaryN",
            "compress", "zoom", "compressTo", "samples", "fast", "fastGap", "density", "slow", "trunc", "densityGap",
            "sparsity", "linger", "segment", "discretise", "timeLoop", "swing", "ply", "iter", "slowstripe", "shuffle",
            "scramble", "repeatCycles", "stripe", "rot", "degradeBy", "unDegradeBy", "mask", "struct", "substruct",
            "distrib", "spaceOut", "chop", "striate", "gap", "randslice", "spin", "hurry", "loopAt", "rolledBy", "jux",
            "juxcut", "jux4", "sometimes", "often", "rarely", "almostNever", "almostAlways", "never", "always",
            "superimpose", "someCycles", "pF", "pN", "pI", "pS", "select", "squeeze", "scale", "toScale", "arp",
            "chooseBy", "wchooseBy", "sseq", "quantise", "cF", "range", "slice", "splice", "chew", "bite", "playFor",
            "swingBy", "wedge", "sew", "while", "euclid", "euclidInv", "degradeOverBy", "spread", "slowspread",
            "fastspread", "spreadChoose", "spreadr", "stitch", "striateBy", "off", "plyWith", "inside", "outside",
            "every", "plyWith", "chunk", "sometimesBy", "someCyclesBy", "plyWith", "juxBy", "fix", "unfix", "foldEvery",
            "within", "fit", "pickF", "weave", "weaveWith", "selectF", "lindenmayer", "rangex", "stutter", "euclidFull",
            "stut", "echo", "stutWith", "echoWith", "whenmod", "contrast", "vib", "vibmod"
        ]),
    }
}

fn mondo() -> Syntax {
    Syntax {
        comment: "//",
        #[rustfmt::skip]
        keywords: BTreeSet::from([ // grep -o "\"[a-zA-Z0-9]\+\"" /srv/codeberg.org/uzu/tidal/tidal-mondo/src/Mondo/Tidal.hs | tr '\n' ',' | sed 's/,/, /g'
            "sound", "cc", "nrpn", "drum", "bank", "midicmd", "toArg", "unit", "vowel", "s", "randrun", "irand", "scan",
            "sine", "square", "cosine", "rand", "perlin", "run", "saw", "tri", "quantise", "arp", "rot", "smooth",
            "trigger", "qtrigger", "qt", "ctrigger", "rtrigger", "ftrigger", "mono", "splitQueries", "rev",
            "filterOnsets", "filterDigital", "filterAnalog", "degrade", "brak", "palindrome", "stretch", "loopFirst",
            "arpeggiate", "arpg", "rolled", "press", "hurry", "loopAt", "ribbon", "beat", "ghost", "fadeOut", "fadeIn",
            "stutter", "weave", "superimpose", "off", "smash", "every", "chunk", "sometimes", "often", "jux", "juxBy",
            "sometimesBy", "someCyclesBy", "slowSqueeze", "sparsity", "fastGap", "densityGap", "fast", "fastSqueeze",
            "density", "slow", "steptake", "stepdrop", "trunc", "linger", "segment", "discretise", "timeLoop", "swing",
            "pressBy", "ply", "reset", "restart", "struct", "mask", "interlace", "add", "ladd", "radd", "sub", "lsub",
            "rsub", "mul", "lmul", "rmul", "div", "ldiv", "rdiv", "mod", "lmod", "rmod", "lvalue", "rvalue",
            "repeatCycles", "iter", "stripe", "slowstripe", "shuffle", "scramble", "chop", "spin", "striate", "gap",
            "randslice", "striateBy", "splice", "euclid", "euclidInv", "slice", "chew", "echo", "accelerate", "amp",
            "attack", "bandf", "bandq", "begin", "binshift", "ccn", "ccv", "clhatdecay", "coarse", "comb", "control",
            "cps", "crush", "ctlNum", "ctranspose", "cutoff", "cutoffegint", "decay", "degree", "delay",
            "delayfeedback", "delaytime", "detune", "distort", "djf", "dry", "dur", "end", "enhance", "expression",
            "fadeInTime", "fadeTime", "frameRate", "frames", "freeze", "freq", "from", "fshift", "fshiftnote",
            "fshiftphase", "gain", "gate", "harmonic", "hatgrain", "hbrick", "hcutoff", "hold", "hours", "hresonance",
            "imag", "kcutoff", "krush", "lagogo", "lbrick", "lclap", "lclaves", "lclhat", "lcrash", "legato", "clip",
            "leslie", "lfo", "lfocutoffint", "lfodelay", "lfoint", "lfopitchint", "lfoshape", "lfosync", "lhitom",
            "lkick", "llotom", "lock", "loop", "lophat", "lrate", "lsize", "lsnare", "metatune", "midibend", "midichan",
            "miditouch", "minutes", "modwheel", "mtranspose", "nudge", "octaveR", "octer", "octersub", "octersubsub",
            "offset", "ophatdecay", "overgain", "overshape", "pan", "panorient", "panspan", "pansplay", "panwidth",
            "partials", "phaserdepth", "phaserrate", "pitch1", "pitch2", "pitch3", "polyTouch", "portamento", "progNum",
            "rate", "real", "release", "resonance", "ring", "ringdf", "ringf", "room", "sagogo", "sclap", "sclaves",
            "scram", "scrash", "seconds", "semitone", "shape", "size", "slide", "smear", "songPtr", "speed", "squiz",
            "stepsPerOctave", "stutterdepth", "stuttertime", "sustain", "sustainpedal", "timescale", "timescalewin",
            "to", "tomdecay", "tremolodepth", "tremolorate", "triode", "tsdelay", "uid", "val", "vcfegint", "vcoegint",
            "velocity", "voice", "waveloss", "xsdelay", "voi", "vco", "vcf", "tremr", "tremdp", "tdecay", "sz", "sus",
            "stt", "std", "sld", "scr", "scp", "scl", "sag", "rel", "por", "pit3", "pit2", "pit1", "phasr", "phasdp",
            "ohdecay", "lsn", "lpq", "lpf", "loh", "llt", "lht", "lfop", "lfoi", "lfoc", "lcr", "lcp", "lcl", "lch",
            "lbd", "lag", "hpq", "hpf", "hg", "gat", "fadeOutTime", "dt", "dfb", "det", "delayt", "delayfb", "ctfg",
            "ctf", "chdecay", "bpq", "bpf", "att", "dec", "lpf", "hpf", "vib", "vib", "vibmod", "vibmod", "distortvol",
            "distortvol", "pw", "pw", "lpe", "lpe", "lpa", "lpa", "lpd", "lpd", "lps", "lps", "lpr", "lpr", "hpe",
            "hpe", "hpa", "hpa", "hpd", "hpd", "hps", "hps", "hpr", "hpr", "bpe", "bpe", "bpa", "bpa", "bpd", "bpd",
            "bps", "bps", "bpr", "bpr", "penv", "penv", "patt", "patt", "pdec", "pdec", "psus", "psus", "prel", "prel",
            "fm", "fm", "fmh", "fmh", "fme", "fme", "fma", "fma", "fmd", "fmd", "fms", "fms", "fmr", "fmr", "am", "am",
            "amdepth", "amdepth", "rm", "rm", "rmdepth", "rmdepth", "phaser", "phaser", "phaserdepth", "phaserdepth",
            "phasersweep", "phasersweep", "phasercenter", "phasercenter", "flanger", "flanger", "flangerdepth",
            "flangerdepth", "flangerfeedback", "flangerfeedback", "chorus", "chorus", "chorusdepth", "chorusdepth",
            "chorusdelay", "chorusdelay", "coarse", "coarse", "crush", "crush", "distort", "distort", "distortvol",
            "distortvol", "delay", "delay", "delaytime", "delaytime", "delayfeedback", "delayfeedback", "verb", "verb",
            "verbdecay", "verbdecay", "verbdamp", "verbdamp", "verbpredelay", "verbpredelay", "verbdiff", "verbdiff",
            "recv", "nrpnn", "nrpnv", "channel", "cut", "octave", "orbit", "midinote", "n", "note", "up", "number",
        ]),
    }
}
