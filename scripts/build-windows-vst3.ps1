$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$target = "x86_64-pc-windows-gnu"
$binDir = Join-Path $repoRoot "bin"

Set-Location $repoRoot

foreach ($tool in @("cargo", "cabal", "ghc", "ghc-pkg")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "Required tool '$tool' was not found on PATH."
    }
}

if (-not (Get-Command "x86_64-w64-mingw32-objdump" -ErrorAction SilentlyContinue) -and
    -not (Get-Command "objdump" -ErrorAction SilentlyContinue)) {
    throw "Install the MinGW-w64 toolchain so objdump can collect the plugin's DLL dependencies."
}

if (Test-Path $binDir) {
    Remove-Item $binDir -Recurse -Force
}
New-Item -ItemType Directory -Path $binDir | Out-Null

Write-Host "Building the Haskell FFI library..."
cabal build -O2
if ($LASTEXITCODE -ne 0) {
    throw "cabal build failed."
}

$ffiLibrary = (& cabal list-bin flib:pluguzu | Select-Object -Last 1).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "Could not query Cabal for the built Haskell foreign library."
}
if (-not (Test-Path $ffiLibrary -PathType Leaf)) {
    # Cabal may report the unoptimized path even though GHC places the
    # linked DLL under its opt/build directory.
    $distRoot = Join-Path $repoRoot "dist-newstyle"
    $ffiCandidate = Get-ChildItem -Path $distRoot -Filter "pluguzu.dll" -File -Recurse |
        Select-Object -First 1
    if ($ffiCandidate) {
        $ffiLibrary = $ffiCandidate.FullName
    }
}
if (-not (Test-Path $ffiLibrary -PathType Leaf)) {
    throw "Could not locate the built Haskell foreign library. Cabal reported: $ffiLibrary"
}
if ([IO.Path]::GetExtension($ffiLibrary) -ne ".dll") {
    throw "Expected a Windows DLL from Cabal, but found: $ffiLibrary"
}

Copy-Item $ffiLibrary $binDir
$ffiDirectory = Split-Path -Parent $ffiLibrary
$importLibraries = Get-ChildItem -Path (Join-Path $repoRoot "dist-newstyle") -Filter "*pluguzu*.dll.a" -File -Recurse
if (-not $importLibraries) {
    throw "Cabal produced no MinGW import library (*.dll.a) beside the Haskell DLL."
}
Copy-Item $importLibraries.FullName $binDir

Write-Host "Bundling the Windows x64 VST3..."
$pkgConfig = Get-Command "pkg-config" -ErrorAction Stop
$mingwPrefix = Split-Path -Parent (Split-Path -Parent $pkgConfig.Source)
$sndfileLibDir = Join-Path $mingwPrefix "lib"
$sndfileLibs = Get-ChildItem -Path $sndfileLibDir -Filter "*sndfile*.a" -File -ErrorAction SilentlyContinue
if (-not $sndfileLibs) {
    throw "Could not find libsndfile import libraries beside MSYS2's pkg-config: $sndfileLibDir"
}
$env:RUSTFLAGS = (@($env:RUSTFLAGS, "-L native=$sndfileLibDir") -join " ").Trim()
cargo xtask bundle pluguzu --release --target $target
if ($LASTEXITCODE -ne 0) {
    throw "The Rust VST3 build failed."
}

$moduleDir = Join-Path $repoRoot "target/bundled/pluguzu.vst3/Contents/x86_64-win"
$pluginModule = Join-Path $moduleDir "pluguzu.vst3"
if (-not (Test-Path $pluginModule)) {
    throw "The VST3 bundler did not create the expected module: $pluginModule"
}

# VST3 hosts load the binary from inside the bundle. Keep the Haskell DLL and
# its runtime DLL dependencies beside it so the plugin does not rely on the
# user's PATH.
$ffiDlls = Get-ChildItem -Path $binDir -Filter "*.dll" -File
Copy-Item $ffiDlls.FullName $moduleDir

$objdump = Get-Command "x86_64-w64-mingw32-objdump" -ErrorAction SilentlyContinue
if (-not $objdump) {
    $objdump = Get-Command "objdump" -ErrorAction Stop
}

$searchDirs = [Collections.Generic.List[string]]::new()
$ghcLibDir = (& ghc --print-libdir | Select-Object -Last 1).Trim()
$searchDirs.Add($moduleDir)
$searchDirs.Add($binDir)
$searchDirs.Add($ffiDirectory)
$searchDirs.Add($ghcLibDir)
$searchDirs.Add((Join-Path $ghcLibDir "bin"))
$searchDirs.Add((Join-Path $ghcLibDir "..\mingw\bin"))
$searchDirs.Add((Join-Path $repoRoot "dist-newstyle"))

$packageDump = & cabal exec -- ghc-pkg dump
if ($LASTEXITCODE -eq 0) {
    foreach ($line in $packageDump) {
        if ($line -match "^dynamic-library-dirs:\s*(.+)$") {
            foreach ($path in $Matches[1].Split(";")) {
                $path = $path.Trim()
                if ($path -and (Test-Path $path -PathType Container)) {
                    $searchDirs.Add($path)
                }
            }
        }
    }
}

foreach ($path in $env:PATH.Split([IO.Path]::PathSeparator)) {
    if ($path -and (Test-Path $path -PathType Container)) {
        $searchDirs.Add($path)
    }
}

$searchDirs = @($searchDirs | Where-Object { $_ -and (Test-Path $_ -PathType Container) } | Select-Object -Unique)
$pending = [Collections.Generic.Queue[string]]::new()
$pending.Enqueue($pluginModule)
foreach ($runtimeDll in Get-ChildItem -Path $moduleDir -Filter "*.dll" -File) {
    $pending.Enqueue($runtimeDll.FullName)
}
$copied = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$system32 = Join-Path $env:WINDIR "System32"

while ($pending.Count -gt 0) {
    $currentDll = $pending.Dequeue()
    $imports = & $objdump.Source -p $currentDll 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "Could not inspect DLL dependencies for '$currentDll'."
    }

    foreach ($line in $imports) {
        if ($line -match "^\s*DLL Name:\s*(.+?)\s*$") {
            $dllName = $Matches[1].Trim()
            if (-not $dllName -or
                $dllName -match "^(?i:api-ms-win-|ext-ms-win-)" -or
                (Test-Path (Join-Path $system32 $dllName))) {
                continue
            }

            $destination = Join-Path $moduleDir $dllName
            if (Test-Path $destination) {
                continue
            }

            $dependency = $null
            foreach ($directory in $searchDirs) {
                $candidate = Join-Path $directory $dllName
                if (Test-Path $candidate -PathType Leaf) {
                    $dependency = $candidate
                    break
                }
            }
            if (-not $dependency) {
                $candidate = Get-ChildItem -Path $repoRoot -Filter $dllName -File -Recurse -ErrorAction SilentlyContinue |
                    Select-Object -First 1
                if ($candidate) {
                    $dependency = $candidate.FullName
                }
            }
            if (-not $dependency) {
                throw "Missing runtime DLL '$dllName', required by '$currentDll'."
            }

            Copy-Item $dependency $destination
            if ($copied.Add($dllName)) {
                $pending.Enqueue($destination)
            }
        }
    }
}

Write-Host "Windows x64 VST3 bundle ready: $moduleDir"
