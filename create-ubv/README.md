# create-ubv

Synthesise a `.ubv` file from a source MP4. Primarily intended for building
deterministic test fixtures for the rest of the workspace, rather than
shipping with real Unifi recordings.

The produced `.ubv` is the minimum viable shape that `ubv::reader` and
`remux_lib` accept: one partition header, one clock-sync record, and the
video track as a sequence of length-prefixed NAL units with SPS/PPS (or
VPS/SPS/PPS for HEVC) injected inline on every keyframe.

## Usage

### CLI

```
cargo build --no-default-features -p create-ubv
./target/debug/create-ubv input.mp4 output.ubv
```

Options:

- `--wall-clock-secs <SECONDS>` — UTC epoch seconds stamped into the clock
  sync record (defaults to `2024-01-01T00:00:00Z` for reproducibility).

### As a library

```toml
[dev-dependencies]
create-ubv = { path = "../create-ubv", default-features = false }
```

```rust
use create_ubv::{synth_from_mp4, SynthConfig};

synth_from_mp4(
    std::path::Path::new("input.mp4"),
    std::path::Path::new("out.ubv"),
    &SynthConfig::default(),
)?;
```

## Features

- **H.264** and **HEVC** MP4 inputs.
- `avcC` / `hvcC` extradata is parsed and SPS/PPS/VPS NAL units are
  prepended to every keyframe so downstream probing (which feeds a raw
  `h264` / `hevc` bitstream to FFmpeg) can discover codec parameters.
- DTS is rescaled from the source stream's timebase to the UBV video
  clock (90 kHz).
- Output round-trips through `ubv::reader::parse_ubv` — the crate has
  unit tests that verify byte-level compatibility of each record type.

## Limitations

- **Video only.** Audio streams in the source MP4 are silently discarded.
  The rest of the pipeline accepts video-only `.ubv` files, so this is
  sufficient for MP4 round-trip smoke tests, but will not exercise audio
  code paths.
- **Single partition.** Real Unifi recordings split into multiple
  partitions at fixed intervals or file-size thresholds; the synthesiser
  emits exactly one.
- **4-byte NAL length prefix only.** The AVC/HEVC length-prefix size is
  read from the MP4's extradata but the crate rejects anything other than
  4 bytes (by far the most common; simplifies keeping the wire format
  byte-identical to UBV's native layout).
- **First packet must be a keyframe.** The MP4 is expected to start with
  a random-access point; `synth_from_mp4` errors otherwise.
- **Fixed wall-clock anchor.** One clock-sync record at the start of the
  partition, nothing periodic. Fine for short fixtures; not intended for
  simulating hours-long drift behaviour.
- **No AV1, no Opus, no JPEG snapshots, no Smart-Event metadata.** The
  real UBV format carries all of these; the synthesiser does not.

## Feature flags

- `ffmpeg-static` (default, on): statically links FFmpeg, compiled from
  source. Disable with `--no-default-features` for fast iteration against
  system FFmpeg via `pkg-config`.

## Regenerating the test fixture

```
ffmpeg -y -f lavfi -i testsrc=size=64x64:rate=30:duration=0.2 \
    -c:v libx264 -profile:v baseline -level 3.0 \
    -g 2 -x264-params keyint=2:scenecut=0 \
    -pix_fmt yuv420p -movflags +faststart \
    tests/fixtures/tiny.mp4
```

Produces a ~3.5 KB MP4 with 6 frames (3 I + 3 P) used by
`tests/roundtrip.rs` and by `remux-lib/tests/e2e_synthetic.rs`.
