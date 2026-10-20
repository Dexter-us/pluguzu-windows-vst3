# Zen Master Windows VST3 experiment

`zen-master` is an experimental branch based on the `Pluguzu` Windows-port
repository's `main` branch. The Windows editor uses CodeMirror 6 inside a Wry /
WebView2 child window; if WebView2 cannot be created, it falls back to the
existing egui editor.

## Build on Windows

Install Node.js 24, Rust, GHC/Cabal, and the MSYS2 MINGW64 toolchain used by the
existing Windows VST3 build. Then build the editor page and plugin:

```powershell
cd web-ui
npm ci --no-audit --no-fund
npm run build
cd ..
.\scripts\build-windows-vst3.ps1
```

The resulting `target/bundled/zen-master.vst3` bundle can be installed alongside
the existing Pluguzu bundle. The editor uses the Microsoft Edge WebView2
runtime available on the target machine. The custom Open and Save dialogs stay
inside the plugin editor; no native file picker is opened.

The GitHub Actions workflow builds this branch and publishes the VST3 bundle as
the `zen-master-windows-x64-vst3` workflow artifact.
