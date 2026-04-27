# create-ubv

Synthesise a `.ubv` file from a source MP4. Primarily intended for building
deterministic test fixtures for the rest of the workspace, rather than
shipping with real Unifi recordings.

The produced `.ubv` is the minimum viable shape that `ubv::reader` and
`remux_lib` accept: one partition header, one clock-sync record, and the
video track. For H.264/HEVC the video is a sequence of length-prefixed
NAL units with SPS/PPS (or VPS/SPS/PPS) injected inline on every
keyframe; for AV1 it is Low Overhead Bitstream Format OBUs with a
Temporal Delimiter prepended to every frame and the Sequence Header
inlined on every keyframe.

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

- **H.264**, **HEVC** and **AV1** MP4 inputs.
- `avcC` / `hvcC` / `av1C` extradata is parsed and codec parameter sets
  are prepended to every keyframe — SPS/PPS for H.264, VPS/SPS/PPS for
  HEVC, the Sequence Header OBU for AV1 — so downstream probing (which
  feeds a raw `h264` / `hevc` / `obu` bitstream to FFmpeg) can discover
  codec parameters.
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
- **No Opus, no JPEG snapshots, no Smart-Event metadata.** The real UBV
  format carries all of these; the synthesiser does not.

## Feature flags

- `ffmpeg-static` (default, on): statically links FFmpeg, compiled from
  source. Disable with `--no-default-features` for fast iteration against
  system FFmpeg via `pkg-config`.

## Regenerating the reference fixtures

Two fixtures live in `testdata/essence/`, exercised by `ubv::reference_testsrc2*`
and `remux-lib::reference_testsrc2*`:

- `testsrc2.ubv` — H.264 + AAC, 640x480 @ 30 fps for 5 s (covers A/V interleave).
- `testsrc2_av1.ubv` — AV1 video-only, 320x240 @ 30 fps for 2 s (covers the AV1
  OBU plumbing).

Both are produced by feeding an ffmpeg `testsrc2` MP4 through this crate; see
`scripts/create-test-ubv-round-trip.sh` for the MP4-generation recipe (pass
`--codec av1` to switch to the AV1 variant).

After regenerating either `.ubv`, refresh the paired JSON snapshot:

```
cargo run -p ubv-info -- --json testdata/essence/testsrc2.ubv \
    > testdata/essence/testsrc2.json
cargo run -p ubv-info -- --json testdata/essence/testsrc2_av1.ubv \
    > testdata/essence/testsrc2_av1.json
```

The remux-lib reference test also asserts the md5 of the demuxed elementary
stream — when the encoded bytes change (e.g. a new libsvtav1 / libx264
version), update the `EXPECTED_*_MD5` constants in the corresponding
`remux-lib/tests/reference_testsrc2*.rs`.
