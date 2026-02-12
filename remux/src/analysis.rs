use std::io;

use chrono::{DateTime, TimeZone, Utc};
use ubv::clock::wc_ticks_to_millis;
use ubv::frame::RecordHeader;
use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_audio_track, is_video_track};

/// Analysed metadata for a single track within a partition.
#[derive(Debug, Clone)]
pub struct AnalysedTrack {
    pub track_id: u16,
    pub frame_count: u32,
    /// UBV clock rate in Hz (e.g. 90000 for video). Becomes the MP4 timescale.
    pub clock_rate: u32,
    /// Informational framerate (video) or sample rate (audio), for logging/timecode.
    pub nominal_fps: u32,
    /// Per-frame DTS values rebased so dts_values[0] == 0, in clock_rate units.
    pub dts_values: Vec<u64>,
    pub start_timecode: Option<DateTime<Utc>>,
}

/// Analysed partition with resolved tracks and demux-ready frame list.
#[derive(Debug, Clone)]
pub struct AnalysedPartition {
    pub video_track_count: u32,
    pub audio_track_count: u32,
    pub video_track: Option<AnalysedTrack>,
    pub audio_track: Option<AnalysedTrack>,
    pub frames: Vec<RecordHeader>,
}

/// Convert a wall-clock value (in track clock_rate units) to a UTC DateTime.
fn wc_to_datetime(wc: u64, clock_rate: u32) -> io::Result<DateTime<Utc>> {
    let utc_millis = wc_ticks_to_millis(wc, clock_rate) as i64;
    let secs = utc_millis / 1000;
    let nanos = ((utc_millis % 1000) * 1_000_000) as u32;
    Utc.timestamp_opt(secs, nanos).single().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("wall-clock timestamp out of range: secs={secs} nanos={nanos}"),
        )
    })
}

/// Compute a nominal framerate from DTS deltas using the median for outlier robustness.
pub fn compute_nominal_fps(dts_values: &[u64], clock_rate: u32) -> u32 {
    if dts_values.len() < 2 || clock_rate == 0 {
        return 1;
    }

    let mut deltas: Vec<u64> = dts_values
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]))
        .filter(|&d| d > 0)
        .collect();

    if deltas.is_empty() {
        return 1;
    }

    deltas.sort_unstable();
    let median = deltas[deltas.len() / 2];

    if median == 0 {
        return 1;
    }

    let fps = (clock_rate as u64 + median / 2) / median; // rounded division
    (fps as u32).max(1)
}

/// Generate a non-drop-frame timecode string (HH:MM:SS:FF) from a start time and framerate.
pub fn generate_timecode(start: &DateTime<Utc>, framerate: u32) -> String {
    let hms = start.format("%H:%M:%S");
    let nanos = start.timestamp_subsec_nanos() as u64;
    let frame = ((nanos * framerate as u64 + 500_000_000) / 1_000_000_000 + 1).min(framerate as u64);
    format!("{}:{:02}", hms, frame)
}

/// Analyse a parsed UBV partition, extracting track metadata, per-frame DTS values,
/// and building the demux frame list.
///
/// Track selection policy:
/// - Video: only frames for `video_track_num`
/// - Audio: all tracks where `is_audio_track(track_id)` is true, when `extract_audio` is enabled
///
/// The returned `audio_track` summary currently represents the first audio track encountered.
pub fn analyse(
    partition: &Partition,
    extract_audio: bool,
    video_track_num: u16,
) -> io::Result<AnalysedPartition> {
    struct TrackState {
        track_id: u16,
        is_video: bool,
        frame_count: u32,
        clock_rate: u32,
        dts_values: Vec<u64>,
        start_timecode: Option<DateTime<Utc>>,
    }

    let mut tracks: Vec<TrackState> = Vec::new();
    let mut frames: Vec<RecordHeader> = Vec::new();
    let mut video_track_count = 0u32;
    let mut audio_track_count = 0u32;

    fn find_or_create_track(
        tracks: &mut Vec<TrackState>,
        track_id: u16,
        is_video: bool,
        video_count: &mut u32,
        audio_count: &mut u32,
    ) -> usize {
        if let Some(idx) = tracks.iter().position(|t| t.track_id == track_id) {
            return idx;
        }
        if is_video {
            *video_count += 1;
        } else {
            *audio_count += 1;
        }
        tracks.push(TrackState {
            track_id,
            is_video,
            frame_count: 0,
            clock_rate: 0,
            dts_values: Vec::new(),
            start_timecode: None,
        });
        tracks.len() - 1
    }

    for entry in &partition.entries {
        let frame = match entry {
            PartitionEntry::Frame(f) => f,
            _ => continue,
        };

        let is_video = is_video_track(frame.header.track_id);
        let is_audio = is_audio_track(frame.header.track_id);

        // Only process the requested video track and known audio tracks.
        if is_video && frame.header.track_id != video_track_num {
            continue;
        }
        if is_audio && !extract_audio {
            continue;
        }
        if !is_video && !is_audio {
            continue;
        }

        let idx = find_or_create_track(
            &mut tracks,
            frame.header.track_id,
            is_video,
            &mut video_track_count,
            &mut audio_track_count,
        );
        let track = &mut tracks[idx];

        // Record clock_rate from the first frame
        if track.frame_count == 0 && frame.header.clock_rate > 0 {
            track.clock_rate = frame.header.clock_rate;
        }

        // Compute timecode from wall-clock on first frame
        if frame.header.clock_rate > 0 && track.frame_count == 0 {
            let dt = wc_to_datetime(frame.wc, frame.header.clock_rate)?;
            track.start_timecode = Some(dt);
            if track.is_video {
                log::info!("First Frame: {}", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"));
            }
        }

        // Collect raw DTS for every frame
        track.dts_values.push(frame.header.dts);

        track.frame_count += 1;
        frames.push(frame.header);
    }

    // Rebase DTS values so first frame = 0, and build AnalysedTrack
    let to_analysed = |t: &TrackState| -> AnalysedTrack {
        let rebased: Vec<u64> = if t.dts_values.is_empty() {
            Vec::new()
        } else {
            let base = t.dts_values[0];
            t.dts_values.iter().map(|&d| d.saturating_sub(base)).collect()
        };

        let nominal_fps = if t.is_video {
            compute_nominal_fps(&rebased, t.clock_rate)
        } else {
            // Audio: nominal_fps = clock_rate (sample rate) for informational purposes
            t.clock_rate
        };

        AnalysedTrack {
            track_id: t.track_id,
            frame_count: t.frame_count,
            clock_rate: t.clock_rate,
            nominal_fps,
            dts_values: rebased,
            start_timecode: t.start_timecode,
        }
    };

    let video_track = tracks
        .iter()
        .find(|t| t.is_video && t.track_id == video_track_num)
        .map(&to_analysed);

    let audio_track = tracks
        .iter()
        // Authoritative summary policy: first encountered audio track.
        .find(|t| !t.is_video)
        .map(&to_analysed);

    Ok(AnalysedPartition {
        video_track_count,
        audio_track_count,
        video_track,
        audio_track,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_generate_timecode() {
        let dt = Utc.with_ymd_and_hms(2023, 5, 12, 10, 30, 45).unwrap();
        let tc = generate_timecode(&dt, 30);
        assert_eq!(tc, "10:30:45:01");
    }

    #[test]
    fn test_generate_timecode_with_nanos() {
        // 500ms at 30fps = frame 16 (0.5 * 30 + 1 = 16)
        let dt = Utc
            .with_ymd_and_hms(2023, 5, 12, 10, 0, 0)
            .unwrap()
            .checked_add_signed(chrono::Duration::milliseconds(500))
            .unwrap();
        let tc = generate_timecode(&dt, 30);
        assert_eq!(tc, "10:00:00:16");
    }

    #[test]
    fn test_generate_timecode_large_nanos() {
        let dt = Utc
            .with_ymd_and_hms(2023, 1, 1, 12, 0, 0)
            .unwrap()
            .checked_add_signed(chrono::Duration::nanoseconds(933_333_333))
            .unwrap();
        let tc = generate_timecode(&dt, 60);
        assert_eq!(tc, "12:00:00:57");
    }

    #[test]
    fn test_generate_timecode_boundary() {
        // 999,999,999ns at 30fps should produce frame 30 (not 31)
        let dt = Utc
            .with_ymd_and_hms(2023, 1, 1, 12, 0, 0)
            .unwrap()
            .checked_add_signed(chrono::Duration::nanoseconds(999_999_999))
            .unwrap();
        let tc = generate_timecode(&dt, 30);
        assert_eq!(tc, "12:00:00:30");
    }

    #[test]
    fn test_wc_to_datetime() {
        let dt = wc_to_datetime(151548043939920, 90000).unwrap();
        assert_eq!(dt.timestamp_millis(), 1683867154888);
    }

    #[test]
    fn test_compute_nominal_fps_30fps() {
        // 90000 Hz clock, 30fps => delta = 3000 ticks
        let dts: Vec<u64> = (0..100).map(|i| i * 3000).collect();
        assert_eq!(compute_nominal_fps(&dts, 90000), 30);
    }

    #[test]
    fn test_compute_nominal_fps_15fps_jittery() {
        // 90000 Hz clock, 15fps => nominal delta = 6000 ticks, add jitter
        let mut dts: Vec<u64> = Vec::new();
        let mut t = 0u64;
        for i in 0..100 {
            dts.push(t);
            // Alternate between 5998, 6000, 6002
            t += 6000 + (i % 3) as u64 - 1;
        }
        assert_eq!(compute_nominal_fps(&dts, 90000), 15);
    }

    #[test]
    fn test_compute_nominal_fps_single_frame() {
        let dts: Vec<u64> = vec![0];
        assert_eq!(compute_nominal_fps(&dts, 90000), 1);
    }

    #[test]
    fn test_compute_nominal_fps_empty() {
        let dts: Vec<u64> = vec![];
        assert_eq!(compute_nominal_fps(&dts, 90000), 1);
    }

    /// Helper: parse a .ubv.gz testdata file and run analysis on the first partition.
    fn analyse_testdata(filename: &str) -> Option<AnalysedPartition> {
        use std::path::Path;
        use ubv::reader::{open_ubv, parse_ubv};
        use ubv::partition::PartitionEntry;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("testdata")
            .join(filename);
        if !path.exists() {
            eprintln!("Skipping test: file not found at {}", path.display());
            return None;
        }

        let mut reader = open_ubv(&path).expect("failed to open UBV file");
        let ubv_file = parse_ubv(&mut reader).expect("failed to parse UBV file");
        assert!(!ubv_file.partitions.is_empty());

        let video_track_id = ubv_file.partitions[0]
            .entries
            .iter()
            .find_map(|e| match e {
                PartitionEntry::Frame(f) if is_video_track(f.header.track_id) => {
                    Some(f.header.track_id)
                }
                _ => None,
            })
            .expect("expected a video track");

        Some(analyse(&ubv_file.partitions[0], true, video_track_id).expect("analysis failed"))
    }

    fn assert_track_invariants(track: &AnalysedTrack, expect_video: bool) {
        assert_eq!(track.dts_values.len() as u32, track.frame_count);
        if track.frame_count == 0 {
            return;
        }
        assert_eq!(track.dts_values[0], 0, "first DTS should be rebased to 0");
        for w in track.dts_values.windows(2) {
            assert!(w[1] >= w[0], "DTS not monotonic: {} < {}", w[1], w[0]);
        }
        if expect_video {
            assert_eq!(track.clock_rate, 90000);
            assert!(
                track.nominal_fps >= 1 && track.nominal_fps <= 240,
                "nominal_fps out of range: {}",
                track.nominal_fps
            );
        }
    }

    #[test]
    fn test_analysis_sample1_h264() {
        let a = match analyse_testdata("sample1_0_rotating_1770769558568.ubv.gz") {
            Some(a) => a,
            None => return,
        };
        let vt = a.video_track.as_ref().expect("expected video track");
        assert_track_invariants(vt, true);
        assert!(vt.frame_count > 0);
        assert_eq!(vt.track_id, 7);
        if let Some(ref at) = a.audio_track {
            assert_track_invariants(at, false);
            assert!(at.clock_rate > 0);
        }
    }

    #[test]
    fn test_analysis_sample2_h264() {
        let a = match analyse_testdata("sample2_0_rotating_1683867159535.ubv.gz") {
            Some(a) => a,
            None => return,
        };
        let vt = a.video_track.as_ref().expect("expected video track");
        assert_track_invariants(vt, true);
        assert!(vt.frame_count > 0);
        assert_eq!(vt.track_id, 7);
        if let Some(ref at) = a.audio_track {
            assert_track_invariants(at, false);
        }
    }

    #[test]
    fn test_analysis_sample3_hevc() {
        let a = match analyse_testdata("sample3_0_rotating_1770695988380.ubv.gz") {
            Some(a) => a,
            None => return,
        };
        let vt = a.video_track.as_ref().expect("expected video track");
        assert_track_invariants(vt, true);
        assert!(vt.frame_count > 0);
        assert_eq!(vt.track_id, 1003);
        if let Some(ref at) = a.audio_track {
            assert_track_invariants(at, false);
        }
    }
}
