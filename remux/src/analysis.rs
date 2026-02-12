use std::io;

use chrono::{DateTime, TimeZone, Utc};
use ubv::clock::wc_ticks_to_millis;
use ubv::frame::RecordHeader;
use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_audio_track, is_video_track};

/// Maximum acceptable framerate (fps) for both probed and bitstream-detected rates.
pub const MAX_ACCEPTED_FPS: u32 = 240;

/// Number of frames to sample when probing video framerate from wall-clock deltas.
const RATE_PROBE_WINDOW: usize = 32;

/// Analysed metadata for a single track within a partition.
#[derive(Debug, Clone)]
pub struct AnalysedTrack {
    pub track_id: u16,
    pub frame_count: u32,
    /// Framerate (video) or sample rate (audio).
    pub rate: u32,
    pub start_timecode: Option<DateTime<Utc>>,
    /// Wall-clock nanos since epoch for the first frame (for A/V sync).
    pub start_nanos: i64,
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

/// Guess the most frequent framerate from a probe window of per-frame rate estimates.
fn guess_video_rate(window: &[u32]) -> u32 {
    let mut best_val = 0u32;
    let mut best_count = 0u32;
    // Simple O(n^2) frequency count — window is at most 32 entries
    let mut counts: Vec<(u32, u32)> = Vec::new();
    for &val in window {
        if val == 0 {
            continue;
        }
        if let Some(entry) = counts.iter_mut().find(|(v, _)| *v == val) {
            entry.1 += 1;
            if entry.1 > best_count {
                best_count = entry.1;
                best_val = val;
            }
        } else {
            counts.push((val, 1));
            if 1 > best_count {
                best_count = 1;
                best_val = val;
            }
        }
    }
    best_val
}

/// Generate a non-drop-frame timecode string (HH:MM:SS:FF) from a start time and framerate.
pub fn generate_timecode(start: &DateTime<Utc>, framerate: u32) -> String {
    let hms = start.format("%H:%M:%S");
    let nanos = start.timestamp_subsec_nanos() as u64;
    let frame = ((nanos * framerate as u64 + 500_000_000) / 1_000_000_000 + 1).min(framerate as u64);
    format!("{}:{:02}", hms, frame)
}

/// Analyse a parsed UBV partition, extracting track metadata, framerate, and
/// building the demux frame list. Only includes frames for the requested
/// video track and audio track 1000.
pub fn analyse(
    partition: &Partition,
    extract_audio: bool,
    video_track_num: u16,
) -> io::Result<AnalysedPartition> {
    // Per-track state for framerate probing
    struct TrackState {
        track_id: u16,
        is_video: bool,
        frame_count: u32,
        rate: u32,
        rate_probe_window: [u32; RATE_PROBE_WINDOW],
        rate_probe_last_wc: u64,
        start_timecode: Option<DateTime<Utc>>,
        start_nanos: i64,
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
            rate: 0,
            rate_probe_window: [0; 32],
            rate_probe_last_wc: 0,
            start_timecode: None,
            start_nanos: 0,
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

        // Only process the requested video track and audio
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

        // Compute timecode from wall-clock
        if frame.header.clock_rate > 0 {
            let dt = wc_to_datetime(frame.wc, frame.header.clock_rate)?;

            if track.frame_count == 0 {
                // First frame
                track.start_timecode = Some(dt);
                track.start_nanos = (frame.wc as u128 * 1_000_000_000 / frame.header.clock_rate as u128) as i64;

                if !track.is_video {
                    // Audio: rate = clock_rate (sample rate)
                    track.rate = frame.header.clock_rate;
                } else {
                    log::info!("First Frame: {}", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"));
                    track.rate_probe_last_wc = frame.wc;
                }
            } else if track.rate == 0 && track.is_video {
                let fc = track.frame_count as usize;
                if fc < track.rate_probe_window.len() {
                    let delta = frame.wc.saturating_sub(track.rate_probe_last_wc);
                    if delta > 0 {
                        let rate = frame.header.clock_rate as u64 / delta;
                        track.rate_probe_window[fc] = rate.min(u32::MAX as u64) as u32;
                    }
                    track.rate_probe_last_wc = frame.wc;
                } else if fc == track.rate_probe_window.len() {
                    let rate = guess_video_rate(&track.rate_probe_window);
                    if rate > 0 && rate < MAX_ACCEPTED_FPS {
                        track.rate = rate;
                        log::info!(
                            "Video Rate Probe: File appears to be {} fps. Use --force-rate if incorrect.",
                            rate
                        );
                    } else if rate == 0 {
                        log::warn!("Video Rate Probe: probed rate was 0 fps. Assuming timelapse file and using 1fps");
                        track.rate = 1;
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Video Rate Probe: probed rate was {} fps. Assuming invalid. Please use --force-rate",
                                rate
                            ),
                        ));
                    }
                }
            }

        }

        track.frame_count += 1;

        frames.push(frame.header);
    }

    // Build final analysed tracks
    let to_analysed = |t: &TrackState| AnalysedTrack {
        track_id: t.track_id,
        frame_count: t.frame_count,
        rate: t.rate,
        start_timecode: t.start_timecode,
        start_nanos: t.start_nanos,
    };

    let video_track = tracks
        .iter()
        .find(|t| t.is_video && t.track_id == video_track_num)
        .map(&to_analysed);

    let audio_track = tracks
        .iter()
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
    fn test_guess_video_rate() {
        let mut window = [0u32; 32];
        // Simulate mostly 30fps with some noise
        for i in 1..32 {
            window[i] = 30;
        }
        window[5] = 29;
        window[10] = 31;
        assert_eq!(guess_video_rate(&window), 30);
    }

    #[test]
    fn test_generate_timecode_large_nanos() {
        // 999_999_000 nanos at 60fps: exact frame = 999_999_000/1e9 * 60 + 1 = 60.999940 + 1
        // With f32 this loses precision; with f64 the frame number is correct.
        // 933ms at 60fps = floor(0.933333333 * 60) + 1 = 56 + 1 = 57
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
        // 1683867154888 ms = known test value from clock.rs tests
        // wc in 90000Hz units: 151548043939920
        // utc_millis = 151548043939920 * 1000 / 90000 = 1683867154888
        let dt = wc_to_datetime(151548043939920, 90000).unwrap();
        assert_eq!(dt.timestamp_millis(), 1683867154888);
    }
}
