#!/usr/bin/env pwsh
# Build script for Windows (x64)
# Builds remux-ffi native library and C# Avalonia GUI

param(
    [string]$Configuration = "Release",
    [string]$RuntimeIdentifier = "win-x64"
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$NativeDir = Join-Path $RepoRoot "remux-gui-cs" "RemuxGui" "native" $RuntimeIdentifier

Write-Host "=== Building remux-ffi (Rust) ==="
Push-Location $RepoRoot
try {
    cargo build --release --no-default-features -p remux-ffi
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed" }
} finally {
    Pop-Location
}

Write-Host "=== Copying native libraries ==="
New-Item -ItemType Directory -Force $NativeDir | Out-Null
Copy-Item (Join-Path $RepoRoot "target" "release" "remux_ffi.dll") $NativeDir -Force

# Copy FFmpeg DLLs if FFMPEG_DIR is set
if ($env:FFMPEG_DIR) {
    Write-Host "Copying FFmpeg DLLs from $env:FFMPEG_DIR..."
    Copy-Item (Join-Path $env:FFMPEG_DIR "bin" "*.dll") $NativeDir -Force
}

Write-Host "=== Building C# GUI ==="
$CsProject = Join-Path $RepoRoot "remux-gui-cs" "RemuxGui" "RemuxGui.csproj"
dotnet publish $CsProject -c $Configuration -r $RuntimeIdentifier --self-contained -o (Join-Path $RepoRoot "publish" $RuntimeIdentifier)
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed" }

Write-Host "=== Done ==="
Write-Host "Output: $(Join-Path $RepoRoot "publish" $RuntimeIdentifier)"
