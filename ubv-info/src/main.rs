use std::io::Write;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use clap::{Parser, ValueEnum};
use flate2::Compression;
use flate2::write::GzEncoder;
use ubv::clock::{compute_nominal_fps, wc_ticks_to_millis};
use ubv::frame::Frame;
use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_audio_track, is_video_track, track_info};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum TextMode {
    /// Per-partition / per-section summary (default).
    Default,
    /// ubnt_ubvinfo format.
    Legacy,
}

#[derive(Parser)]
#[command(name = "ubv-info", about = "Parse and display UBV file structure")]
struct Args {
    /// Input .ubv file
    #[arg(short = 'f', long = "file")]
    file: Option<String>,

    /// Input .ubv file (positional)
    #[arg(conflicts_with = "file", required_unless_present_any = ["file", "schema", "version"])]
    input: Option<String>,

    /// Filter by track ID (only valid with --json)
    #[arg(short = 't', long = "track", requires = "json")]
    track_filter: Option<u16>,

    /// Output as JSON
    #[arg(long, conflicts_with = "inspect")]
    json: bool,

    /// Write a .metadata.json.gz inspection file
    #[arg(long, conflicts_with = "json")]
    inspect: bool,

    /// Text output style.
    #[arg(long = "text", value_enum, default_value_t = TextMode::Default)]
    text: TextMode,

    /// Maximum wall-clock discontinuity (in seconds) tolerated within a section.
    /// A video frame whose wall-clock is more than this far from the previous
    /// video frame starts a new section. Audio gaps do not split sections.
    #[arg(long = "max-discontinuity", default_value_t = 5.0, value_parser = parse_positive_seconds)]
    max_discontinuity: f64,

    /// Print JSON schema for the output format and exit
    #[arg(long)]
    schema: bool,

    /// Display version and quit
    #[arg(long)]
    version: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Reset SIGPIPE to default so piped output (e.g. head/tail) exits cleanly
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args = Args::parse();

    if args.version {
        ubv::version::print_cli_version_banner(
            "UBV Info Tool",
            env!("GIT_VERSION"),
            env!("GIT_COMMIT"),
        );
        return Ok(());
    }

    if args.schema {
        let schema = schemars::schema_for!(ubv::reader::UbvFile);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    let file = args.file.or(args.input).expect("file argument required");
    let mut reader = ubv::reader::open_ubv(Path::new(&file))
        .map_err(|e| format!("{}: error opening file: {}", file, e))?;
    let ubv = ubv::reader::parse_ubv(&mut reader)
        .map_err(|e| format!("{}: error parsing UBV: {}", file, e))?;

    if args.json {
        println!("{}", serde_json::to_string(&ubv)?);
        return Ok(());
    }

    if args.inspect {
        let output_path = format!("{}.metadata.json.gz", file);
        let json = serde_json::to_string(&ubv)?;
        let out_file = std::fs::File::create(&output_path)
            .map_err(|e| format!("{}: error creating file: {}", output_path, e))?;
        let mut encoder = GzEncoder::new(out_file, Compression::best());
        encoder
            .write_all(json.as_bytes())
            .map_err(|e| format!("{}: error writing: {}", output_path, e))?;
        encoder
            .finish()
            .map_err(|e| format!("{}: error finishing gzip: {}", output_path, e))?;
        eprintln!("Wrote {}", output_path);
        return Ok(());
    }

    match args.text {
        TextMode::Legacy => print_legacy(&ubv),
        TextMode::Default => {
            let max_ms = (args.max_discontinuity * 1000.0) as u64;
            print_summary(&ubv, max_ms);
        }
    }

    Ok(())
}

fn parse_positive_seconds(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!("must be a positive, finite number (got {})", s));
    }
    Ok(v)
}

fn print_legacy(ubv: &ubv::reader::UbvFile) {
    // Print header
    println!(
        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
        "Type", "TID", "KF", "OFFSET", "SIZE", "DTS", "CTS", "WC", "CR", ""
    );

    for partition in &ubv.partitions {
        println!("----------- PARTITION START -----------");

        let mut prev_wc_ms: Option<i64> = None;

        for entry in &partition.entries {
            match entry {
                PartitionEntry::ClockSync(cs) => {
                    println!(
                        "SC: {} ticks @{}Hz -> WC: {}ms",
                        cs.sc_dts, cs.sc_rate, cs.wc_ms
                    );
                }
                PartitionEntry::Frame(frame) => {
                    let wc_ms = if frame.header.clock_rate > 0 {
                        wc_ticks_to_millis(frame.wc, frame.header.clock_rate) as i64
                    } else {
                        0
                    };
                    let delta_ms = match prev_wc_ms {
                        Some(prev) => wc_ms - prev,
                        None => 0,
                    };
                    prev_wc_ms = Some(wc_ms);

                    println!(
                        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
                        format!("{}", frame.type_char),
                        frame.header.track_id,
                        if frame.header.keyframe { 1 } else { 0 },
                        frame.header.data_offset,
                        frame.header.data_size,
                        frame.header.dts,
                        frame.cts,
                        frame.wc,
                        frame.header.clock_rate,
                        delta_ms,
                    );
                }
                PartitionEntry::Motion(m)
                | PartitionEntry::SmartEvent(m)
                | PartitionEntry::Jpeg(m)
                | PartitionEntry::Skip(m)
                | PartitionEntry::Talkback(m) => {
                    let type_char = match entry {
                        PartitionEntry::Motion(_) => "M",
                        PartitionEntry::SmartEvent(_) => "E",
                        PartitionEntry::Jpeg(_) => "J",
                        PartitionEntry::Skip(_) => "S",
                        PartitionEntry::Talkback(_) => "T",
                        _ => unreachable!(),
                    };

                    println!(
                        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
                        type_char,
                        m.header.track_id,
                        "",
                        m.header.data_offset,
                        m.header.data_size,
                        m.header.dts,
                        "",
                        "",
                        m.header.clock_rate,
                        "",
                    );
                }
                _ => {}
            }
        }
    }
}

/// One run of contiguous media within a partition.
///
/// `*_first_ms` / `*_last_ms` track the minimum and maximum wall-clock ms
/// observed for each stream; `last - first` is therefore always a non-negative
/// span even if individual wall-clock values are non-monotonic.
#[derive(Default)]
struct Section {
    /// 1-based section index within the partition.
    index: u32,
    video_first_ms: Option<u64>,
    video_last_ms: Option<u64>,
    audio_first_ms: Option<u64>,
    audio_last_ms: Option<u64>,
    video_frames: u32,
    audio_frames: u32,
    /// Sum of (gap - expected) for video deltas in (2*expected, max_discontinuity_ms], in ms.
    discontinuity_ms: u64,
}

impl Section {
    fn new(index: u32) -> Self {
        Self {
            index,
            ..Self::default()
        }
    }

    /// Earliest wall-clock ms across video and audio in this section.
    fn start_ms(&self) -> Option<u64> {
        min_opt(self.video_first_ms, self.audio_first_ms)
    }

    /// Latest wall-clock ms across video and audio in this section.
    fn end_ms(&self) -> Option<u64> {
        max_opt(self.video_last_ms, self.audio_last_ms)
    }
}

fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn max_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn print_summary(ubv: &ubv::reader::UbvFile, max_discontinuity_ms: u64) {
    let mut first = true;
    for (partition_idx, partition) in ubv.partitions.iter().enumerate() {
        let video_fps = partition_video_fps(partition);
        let sections = build_sections(partition, max_discontinuity_ms, video_fps);
        let video_codec = partition_video_codec(partition);

        if sections.is_empty() {
            if !first {
                println!();
            }
            first = false;
            println!("PARTITION {}:1", partition_idx);
            println!("Start: -");
            println!("End: -");
            println!("Video Codec: none");
            println!("Video Duration: 00:00:00.000");
            println!("Audio Duration: 00:00:00.000");
            continue;
        }

        for sec in &sections {
            if !first {
                println!();
            }
            first = false;

            println!("PARTITION {}:{}", partition_idx, sec.index);
            println!(
                "Start: {}",
                format_timecode(sec.start_ms().and_then(ms_to_datetime), video_fps)
            );
            println!(
                "End: {}",
                format_timecode(sec.end_ms().and_then(ms_to_datetime), video_fps)
            );
            println!("Video Codec: {}", video_codec.as_deref().unwrap_or("none"));
            println!(
                "Video Duration: {}",
                format_duration_ms(
                    sec.video_last_ms
                        .zip(sec.video_first_ms)
                        .map(|(b, a)| b.saturating_sub(a))
                        .unwrap_or(0)
                )
            );
            println!(
                "Audio Duration: {}",
                format_duration_ms(
                    sec.audio_last_ms
                        .zip(sec.audio_first_ms)
                        .map(|(b, a)| b.saturating_sub(a))
                        .unwrap_or(0)
                )
            );

            if sec.discontinuity_ms > 0 {
                println!(
                    "Discontinuities: {}",
                    format_duration_ms(sec.discontinuity_ms)
                );
            }
        }
    }
}

/// Walk media frames in file order. Section boundaries are driven by video wall-clock gaps
/// greater than `max_discontinuity_ms`; audio frames join whichever section is currently
/// open (creating one if needed, so audio-only partitions yield a single section).
fn build_sections(
    partition: &Partition,
    max_discontinuity_ms: u64,
    video_fps: Option<u32>,
) -> Vec<Section> {
    let expected_delta_ms = video_fps
        .filter(|&f| f > 0)
        .map(|f| (1000 / f as u64).max(1))
        .unwrap_or(0);
    let disc_floor_ms = expected_delta_ms.saturating_mul(2);

    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;

    for entry in &partition.entries {
        let frame = match entry {
            PartitionEntry::Frame(f) => f,
            _ => continue,
        };
        if frame.header.clock_rate == 0 {
            continue;
        }
        let is_video = is_video_track(frame.header.track_id);
        let is_audio = is_audio_track(frame.header.track_id);
        if !is_video && !is_audio {
            continue;
        }
        let wc_ms = wc_ticks_to_millis(frame.wc, frame.header.clock_rate);

        // Video gaps may split the current section before the new frame is added.
        // NOTE: backward wall-clock jumps (from clock-sync corrections) currently
        // saturate to delta=0 and are silently ignored. A large backward jump
        // arguably should split a section; future work could detect and surface
        // those rollbacks to the user.
        if is_video
            && let Some(sec) = current.as_ref()
            && let Some(prev) = sec.video_last_ms
            && wc_ms.saturating_sub(prev) > max_discontinuity_ms
        {
            sections.push(current.take().unwrap());
        }

        let next_idx = sections.len() as u32 + 1;
        let sec = current.get_or_insert_with(|| Section::new(next_idx));

        if is_video {
            if let Some(prev) = sec.video_last_ms {
                let delta = wc_ms.saturating_sub(prev);
                if expected_delta_ms > 0
                    && delta > disc_floor_ms
                    && delta <= max_discontinuity_ms
                {
                    sec.discontinuity_ms += delta - expected_delta_ms;
                }
            }
            sec.video_first_ms = Some(sec.video_first_ms.map_or(wc_ms, |v| v.min(wc_ms)));
            sec.video_last_ms = Some(sec.video_last_ms.map_or(wc_ms, |v| v.max(wc_ms)));
            sec.video_frames += 1;
        } else {
            sec.audio_first_ms = Some(sec.audio_first_ms.map_or(wc_ms, |v| v.min(wc_ms)));
            sec.audio_last_ms = Some(sec.audio_last_ms.map_or(wc_ms, |v| v.max(wc_ms)));
            sec.audio_frames += 1;
        }
    }

    if let Some(s) = current.take() {
        sections.push(s);
    }
    sections
}

fn partition_video_codec(partition: &Partition) -> Option<String> {
    partition.entries.iter().find_map(|e| match e {
        PartitionEntry::Frame(Frame { header, .. }) if is_video_track(header.track_id) => {
            track_info(header.track_id)
                .and_then(|t| t.codec)
                .map(|c| c.codec_name.to_uppercase())
        }
        _ => None,
    })
}

/// Estimate the partition's nominal video framerate from the median DTS delta.
///
/// Uses all video-frame DTS values in the partition. A handful of large cross-section
/// deltas at gap boundaries don't move the median appreciably, so no section-aware
/// filtering is needed; multi-section partitions where each section has a different
/// cadence are inherently ambiguous and we accept whichever cadence wins the median.
fn partition_video_fps(partition: &Partition) -> Option<u32> {
    let mut dts: Vec<u64> = Vec::new();
    let mut clock_rate: u32 = 0;
    for entry in &partition.entries {
        if let PartitionEntry::Frame(f) = entry
            && is_video_track(f.header.track_id)
            && f.header.clock_rate > 0
        {
            if clock_rate == 0 {
                clock_rate = f.header.clock_rate;
            }
            dts.push(f.header.dts);
        }
    }
    compute_nominal_fps(&dts, clock_rate)
}

fn ms_to_datetime(ms: u64) -> Option<DateTime<Utc>> {
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    Utc.timestamp_opt(secs, nanos).single()
}

fn format_timecode(dt: Option<DateTime<Utc>>, fps: Option<u32>) -> String {
    let Some(dt) = dt else {
        return "-".to_string();
    };
    match fps.filter(|&f| f > 0) {
        Some(fps) => {
            let nanos = dt.timestamp_subsec_nanos() as u64;
            let frame =
                ((nanos * fps as u64 + 500_000_000) / 1_000_000_000 + 1).min(fps as u64);
            format!("{}:{:02}@{}", dt.format("%Y-%m-%d %H:%M:%S"), frame, fps)
        }
        None => format!("{}", dt.format("%Y-%m-%d %H:%M:%S")),
    }
}

fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let millis = ms % 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formatting() {
        assert_eq!(format_duration_ms(0), "00:00:00.000");
        assert_eq!(format_duration_ms(1_234), "00:00:01.234");
        assert_eq!(format_duration_ms(3_600_000), "01:00:00.000");
        assert_eq!(format_duration_ms(3_661_500), "01:01:01.500");
    }

    #[test]
    fn timecode_includes_timebase() {
        let dt = Utc.with_ymd_and_hms(2026, 5, 26, 14, 30, 45).unwrap();
        let tc = format_timecode(Some(dt), Some(30));
        assert_eq!(tc, "2026-05-26 14:30:45:01@30");
    }

    #[test]
    fn timecode_omits_timebase_when_unknown() {
        let dt = Utc.with_ymd_and_hms(2026, 5, 26, 14, 30, 45).unwrap();
        assert_eq!(
            format_timecode(Some(dt), None),
            "2026-05-26 14:30:45"
        );
        assert_eq!(
            format_timecode(Some(dt), Some(0)),
            "2026-05-26 14:30:45"
        );
    }

    #[test]
    fn parse_positive_seconds_rejects_zero_and_negative() {
        assert!(parse_positive_seconds("0").is_err());
        assert!(parse_positive_seconds("-1").is_err());
        assert!(parse_positive_seconds("NaN").is_err());
        assert!(parse_positive_seconds("inf").is_err());
        assert_eq!(parse_positive_seconds("5").unwrap(), 5.0);
        assert_eq!(parse_positive_seconds("0.5").unwrap(), 0.5);
    }

    // ---- build_sections tests ----

    use ubv::format::PacketPosition;
    use ubv::frame::RecordHeader;
    use ubv::partition::PartitionHeader;
    use ubv::track::{TRACK_AUDIO, TRACK_VIDEO};

    fn frame_entry(track_id: u16, wc_ms: u64) -> PartitionEntry {
        // clock_rate = 1000 so wc ticks == wall-clock ms.
        PartitionEntry::Frame(Frame {
            type_char: if is_video_track(track_id) { 'V' } else { 'A' },
            header: RecordHeader {
                track_id,
                data_offset: 0,
                data_size: 0,
                dts: wc_ms,
                clock_rate: 1000,
                sequence: 0,
                keyframe: false,
            },
            cts: 0,
            wc: wc_ms,
            packet_position: PacketPosition::Single,
        })
    }

    fn partition_with(entries: Vec<PartitionEntry>) -> Partition {
        Partition {
            index: 0,
            entries,
            header: None::<PartitionHeader>,
        }
    }

    #[test]
    fn splits_on_video_gap_exceeding_threshold() {
        let p = partition_with(vec![
            frame_entry(TRACK_VIDEO, 0),
            frame_entry(TRACK_VIDEO, 33),
            // 10s gap, > max_discontinuity_ms = 5000
            frame_entry(TRACK_VIDEO, 10_033),
            frame_entry(TRACK_VIDEO, 10_066),
        ]);
        let secs = build_sections(&p, 5_000, Some(30));
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].index, 1);
        assert_eq!(secs[0].video_frames, 2);
        assert_eq!(secs[0].video_first_ms, Some(0));
        assert_eq!(secs[0].video_last_ms, Some(33));
        assert_eq!(secs[1].index, 2);
        assert_eq!(secs[1].video_frames, 2);
        assert_eq!(secs[1].video_first_ms, Some(10_033));
        assert_eq!(secs[1].video_last_ms, Some(10_066));
    }

    #[test]
    fn accumulates_sub_threshold_discontinuity() {
        // 30 fps: expected_delta = 33, disc_floor = 66.
        // A 500ms gap is > 66 and <= 5000 → counts (500 - 33 = 467ms).
        let p = partition_with(vec![
            frame_entry(TRACK_VIDEO, 0),
            frame_entry(TRACK_VIDEO, 33),
            frame_entry(TRACK_VIDEO, 533),
            frame_entry(TRACK_VIDEO, 566),
        ]);
        let secs = build_sections(&p, 5_000, Some(30));
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].discontinuity_ms, 500 - 33);
    }

    #[test]
    fn audio_only_partition_yields_one_section() {
        let p = partition_with(vec![
            frame_entry(TRACK_AUDIO, 0),
            frame_entry(TRACK_AUDIO, 20),
            frame_entry(TRACK_AUDIO, 40),
        ]);
        let secs = build_sections(&p, 5_000, None);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].video_frames, 0);
        assert_eq!(secs[0].audio_frames, 3);
        assert_eq!(secs[0].audio_first_ms, Some(0));
        assert_eq!(secs[0].audio_last_ms, Some(40));
        assert_eq!(secs[0].video_first_ms, None);
        assert_eq!(secs[0].video_last_ms, None);
    }

    #[test]
    fn audio_between_video_gap_joins_pre_gap_section() {
        // Audio frames that arrive in file order before the next video frame
        // are attributed to the section currently open at that point.
        let p = partition_with(vec![
            frame_entry(TRACK_VIDEO, 0),
            frame_entry(TRACK_AUDIO, 1_000),
            frame_entry(TRACK_AUDIO, 3_000),
            frame_entry(TRACK_VIDEO, 10_000), // gap → new section
            frame_entry(TRACK_AUDIO, 11_000),
        ]);
        let secs = build_sections(&p, 5_000, Some(30));
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].audio_frames, 2);
        assert_eq!(secs[0].audio_first_ms, Some(1_000));
        assert_eq!(secs[0].audio_last_ms, Some(3_000));
        assert_eq!(secs[1].audio_frames, 1);
        assert_eq!(secs[1].audio_first_ms, Some(11_000));
    }

    #[test]
    fn non_monotonic_video_wc_does_not_underflow_duration() {
        // Backward wall-clock jump (clock-sync correction). The dip should be
        // masked for split purposes (current behaviour) but min/max semantics
        // mean last_ms - first_ms is still a real, non-negative span.
        let p = partition_with(vec![
            frame_entry(TRACK_VIDEO, 0),
            frame_entry(TRACK_VIDEO, 100),
            frame_entry(TRACK_VIDEO, 50), // backward
            frame_entry(TRACK_VIDEO, 200),
        ]);
        let secs = build_sections(&p, 5_000, Some(30));
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].video_first_ms, Some(0));
        assert_eq!(secs[0].video_last_ms, Some(200));
    }

}
