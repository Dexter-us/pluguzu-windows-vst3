# pluguzu

A standalone live coding text editor.

> Discuss: https://club.tidalcycles.org/t/introducing-pluguzu-a-plugin-to-run-tidal-natively-in-a-daw/5758

## Overview and scope

The goal of pluguzu is to provide an easy to use environment to perform live code.
The project presently integrates the [Tidal][tidal] code interpreter,
the [Mondo][mondo] evaluator,
and the [Dough][dough] synth so that it can be used out of the box
without configuring external services.

Pluguzu can be used as a regular text editor, connecting directly to the system's
audio and MIDI ports, but it can also be loaded inside a Digital Audio Workstation
(DAW) as a CLAP plugin (or VST3).

Pluguzu is powered by the [nice-plug][nice-plug] audio plugin framework and the
[egui][egui] graphical user interface.

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

[tidal]: https://tidalcycles.org/
[dough]: https://codeberg.org/uzu/dough
[mondo]: https://codeberg.org/uzu/tidal/pulls/1233
[nice-plug]: https://codeberg.org/RustAudio/nice-plug
[egui]: https://egui.rs

## Supported Platforms

Pluguzu is known to work on the following system:

- [x] Linux x86 (Arch/Debian/Fedora)
- [ ] Windows x64 (VST3 build path added; not yet verified in a DAW)

… with the following host:

- [x] Jack (STANDALONE)
- [x] Carla (VST3), but the windows must not overlap to avoid lag.
- [x] Ardour8 (VST3), select option to "allow plugin to receive keyboard events".
- [x] REAPER (CLAP), select option to "send all keyboard input to plugin".
- [x] BespokeSynth (VST3). Keyboard events for edition doesn't presently work.


## Build Instructions

- Install the system requirements:

```bash
# Install Fedora/CentOS system dependencies
sudo dnf install cmake alsa-lib-devel wayland-devel jack-audio-connection-kit-devel pipewire-jack-audio-connection-kit-libs libsamplerate-devel mesa-libGL-devel libXext-devel libXrandr-devel libXfixes-devel libxkbcommon-x11-devel dbus-devel libsndfile-devel gcc gcc-c++ gmp gmp-devel make ncurses ncurses-compat-libs xz perl pkg-config git

# Install Debian system dependencies
sudo apt-get install cmake libasound2-dev libwayland-dev libjack-dev libsamplerate0-dev libgl-dev libxext-dev libxrandr-dev libxfixes-dev libxkbcommon-x11-dev libdbus-1-dev libsndfile1-dev gcc g++ libgmp-dev libncurses-dev perl pkg-config git pipewire-jack

# Arch
sudo pacman -Sy base-devel make cmake glibc wayland-client libffi alsa-lib pipewire-jack

# Install Haskell cabal
curl --proto '=https' --tlsv1.2 -sSf https://get-ghcup.haskell.org | env BOOTSTRAP_HASKELL_GHC_VERSION=latest BOOTSTRAP_HASKELL_NONINTERACTIVE=1 sh
source ~/.ghcup/env

# Install Rust cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

- Get the sources:

```
git clone https://codeberg.org/TristanCacqueray/pluguzu
cd pluguzu
```

- Build the plugin:

```bash
make
make install
```

- Validate by making sure this command doesn't output a missing dependencies:

```bash
ldd ~/.local/bin/pluguzu                                  | grep "not found"
ldd ~/.clap/pluguzu.clap                                  | grep "not found"
ldd ~/.vst3/pluguzu.vst3/Contents/x86_64-linux/pluguzu.so | grep "not found"
```

- Run the standalone JACK client:

```bash
pluguzu
```

### Windows x64 VST3 (experimental)

The Windows build uses the MinGW-w64 Rust target so Rust can link against
Cabal's Windows foreign-library import archive. Install:

- GHC and Cabal, with `ghc`, `ghc-pkg`, and `cabal` available in PowerShell.
- Rust and the `x86_64-pc-windows-gnu` target.
- MinGW-w64 tools (`gcc`, `objdump`, and `pkg-config`), CMake, and the
  `libsndfile` development package.

From the repository root, run:

```powershell
.\scripts\build-windows-vst3.ps1
```

The script builds the Haskell FFI, bundles the x64 VST3, and copies the
non-system DLL dependencies beside the plugin module. The output is
`target\bundled\pluguzu.vst3`; copy that bundle to your DAW's VST3 folder.
This build path has not yet been confirmed in a Windows DAW, so the first
release still needs a host scan and MIDI/audio smoke test.


## Known issues

Here is a list of known issues and how to fix them:

- Failure to use the ALSA backend `'snd_pcm_hw_params_set_buffer_size' failed with error 'Invalid argument (22)'`.
  Try running the pipewire wrapper to use JACK instead: `pw-jack pluguzu`.

- `pluguzu: error while loading shared libraries: libHStidal-parse-0.0.2`: This happens when tidal is updated through the cabal.project. Try removing the dist files and rebuild the haskell-lib:

```
rm -Rf dist-newstyle
make haskell-lib
```

## Architecture

Here are the important files:

- [PluguzuCore.hs](./src/PluguzuCore.hs) contains the logic to convert tidal pattern into MIDI events.
- [Pluguzu.hs](./Pluguzu.hs) is the foreign library called by the plugin.
- [lib.rs](./src/lib.rs) is the nice-plug implementation.
- [dough.rs](./src/dough.rs) is the dough sampler integration.
- [ui.rs](./src/ui.rs) is the UI definition.
- [pluguzu-presets.md](./pluguzu-presets.md) is the preset library.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.

Pluguzu implements a double buffering logic to render the upcoming events in the background:

- See the `Pluguzu::sync_events_buffer` for the realtime part, and,
- `HaskellRuntime::render` for the background work.

> Note that the implementation is a work in progress to implement a lock less solution
> to ensure a smooth process callback, thus it is kind of ugly at the moment.

Pluguzu is driven by the host transport and it takes care of performing the MIDI and Sound events.

## Changelog

Implemented:

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [x] Parse tidal pattern from the plugin text editor UI.
- [x] Render the midi events and submit them from the plugin DSP.
- [x] Handle transport pause/jump.
- [x] Display status and parse error.
- [x] Update text editor language def to support Haskell syntax.
- [x] Handle MIDI CC.
- [x] Add presets.
- [x] Replace DPF framework with nice-plug.
- [x] Highlight event locations.
- [x] Integrate the dirt sampler.
- [x] Loading code through the command line and file menu.
- [x] Support user-defined presets, e.g. in ~/.config/pluguzu/presets.md.
- [x] Add standalone transport, e.g. without sync to an external clock.
- [x] Support the mondo notation.
- [x] Integrate the dough synth.
- [x] Improve error reporting to the UI.
- [x] MIDI triggers.

Next:

- [ ] Quantize MIDI trigger start time.
- [ ] Add pattern visualizations, e.g. a pianoroll in the background.
- [ ] Samples browser.

Future:

- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
