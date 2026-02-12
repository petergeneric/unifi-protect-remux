# Notes on compiling and FFmpeg

This project uses [FFmpeg](https://ffmpeg.org/) for audio decoding and encoding, via `ffmpeg-next`.
Release builds statically link FFmpeg.
This can be slow, so there are two build profiles:
1. `release` (static FFmpeg)
2. `ci` (shared FFmpeg)


## Release
Production (static FFmpeg, the default):

```
cargo build --release
```

FFmpeg is compiled from source and statically linked into the binary.
No runtime FFmpeg dependency.

### Static build system requirements

The `ffmpeg-sys-next` crate compiles FFmpeg from source, which requires:

- C compiler (`gcc` or `clang`)
- `make`
- `nasm` (x86/x86-64 assembler for optimised codec routines)
- `pkg-config`
- `perl`

macOS:
```
xcode-select --install
brew install nasm pkg-config
```

Debian/Ubuntu:
```
apt-get install build-essential nasm pkg-config libclang-dev
```

## CI
Faster builds by relying on OS shared FFmpeg 8.0 install:

```
cargo build --no-default-features
```

Links against system-installed FFmpeg shared libraries via pkg-config.
Skips compiling FFmpeg from source, significantly faster for iterating.

System FFmpeg dev library requirements:
- macOS: brew install ffmpeg
- Debian/Ubuntu: apt-get install libavcodec-dev libavformat-dev libavutil-dev libswresample-dev libswscale-dev libavdevice-dev libavfilter-dev
