use crate::codec::{self, CodecInfo};

/// Track ID constants.
pub const TRACK_RESERVED: u16 = 1;
pub const TRACK_MOTION: u16 = 5;
pub const TRACK_SKIP: u16 = 6;
pub const TRACK_VIDEO: u16 = 7;
pub const TRACK_PARTITION: u16 = 9;
pub const TRACK_SMART_EVENT: u16 = 10;
pub const TRACK_AUDIO: u16 = 1000;
pub const TRACK_AUDIO_RAW: u16 = 1001;
pub const TRACK_AUDIO_OPUS: u16 = 1002;
pub const TRACK_VIDEO_HEVC: u16 = 1003;
pub const TRACK_VIDEO_AV1: u16 = 1004;
pub const TRACK_TALKBACK: u16 = 1005;
pub const TRACK_JPEG: u16 = 0x4A70;
pub const TRACK_CLOCK_SYNC: u16 = 0xDA7E;

/// All known track IDs.
pub const ALL_TRACK_IDS: &[u16] = &[
    TRACK_RESERVED, TRACK_MOTION, TRACK_SKIP, TRACK_VIDEO, TRACK_PARTITION,
    TRACK_SMART_EVENT, TRACK_AUDIO, TRACK_AUDIO_RAW, TRACK_AUDIO_OPUS,
    TRACK_VIDEO_HEVC, TRACK_VIDEO_AV1, TRACK_TALKBACK, TRACK_JPEG, TRACK_CLOCK_SYNC,
];

/// Track type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    VideoH264,
    VideoHevc,
    VideoAv1,
    AudioAac,
    AudioRaw,
    AudioOpus,
    PartitionHeader,
    ClockSync,
    Skip,
    Motion,
    SmartEvent,
    Jpeg,
    Talkback,
    Reserved,
}

/// Information about a track identified by its track ID.
#[derive(Debug, Clone, Copy)]
pub struct TrackInfo {
    pub track_type: TrackType,
    /// Output type character: "V" for video, "A" for audio, None for non-media.
    pub type_char: Option<char>,
    /// Numeric payload type (1-15 per format doc).
    pub payload_type: u8,
    /// Codec info reference. None for non-media tracks.
    pub codec: Option<&'static CodecInfo>,
}

/// Look up track info by track ID.
pub fn track_info(track_id: u16) -> Option<TrackInfo> {
    let (tt, tc, pt, ci) = match track_id {
        TRACK_VIDEO => (TrackType::VideoH264, Some('V'), 1, Some(&codec::VH264)),
        TRACK_VIDEO_HEVC => (TrackType::VideoHevc, Some('V'), 2, Some(&codec::VH265)),
        TRACK_VIDEO_AV1 => (TrackType::VideoAv1, Some('V'), 3, Some(&codec::VAV1)),
        TRACK_AUDIO => (TrackType::AudioAac, Some('A'), 4, Some(&codec::AAAC)),
        TRACK_AUDIO_RAW => (TrackType::AudioRaw, Some('A'), 5, Some(&codec::AG711A)),
        TRACK_AUDIO_OPUS => (TrackType::AudioOpus, Some('A'), 6, Some(&codec::AOPUS)),
        TRACK_PARTITION => (TrackType::PartitionHeader, None, 0, None),
        TRACK_CLOCK_SYNC => (TrackType::ClockSync, None, 0, None),
        TRACK_SKIP => (TrackType::Skip, None, 0, None),
        TRACK_MOTION => (TrackType::Motion, None, 0, None),
        TRACK_SMART_EVENT => (TrackType::SmartEvent, None, 0, None),
        TRACK_JPEG => (TrackType::Jpeg, None, 0, None),
        TRACK_TALKBACK => (TrackType::Talkback, None, 8, None),
        TRACK_RESERVED => (TrackType::Reserved, None, 0, None),
        _ => return None,
    };
    Some(TrackInfo {
        track_type: tt,
        type_char: tc,
        payload_type: pt,
        codec: ci,
    })
}

impl TrackInfo {
    pub fn is_video(&self) -> bool {
        self.type_char == Some('V')
    }

    pub fn is_audio(&self) -> bool {
        self.type_char == Some('A')
    }
}

/// Returns true if this track ID is a video track.
pub fn is_video_track(track_id: u16) -> bool {
    track_info(track_id).is_some_and(|i| i.is_video())
}

/// Returns true if this track ID is an audio track.
pub fn is_audio_track(track_id: u16) -> bool {
    track_info(track_id).is_some_and(|i| i.is_audio())
}

/// Returns true if this track ID produces media output lines.
pub fn is_media_track(track_id: u16) -> bool {
    track_info(track_id).is_some_and(|i| i.type_char.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_tracks() {
        assert!(is_media_track(TRACK_VIDEO));
        assert!(is_media_track(TRACK_VIDEO_HEVC));
        assert!(is_media_track(TRACK_VIDEO_AV1));
        assert!(is_media_track(TRACK_AUDIO));
        assert!(is_media_track(TRACK_AUDIO_RAW));
        assert!(is_media_track(TRACK_AUDIO_OPUS));
    }

    #[test]
    fn test_non_media_tracks() {
        assert!(!is_media_track(TRACK_PARTITION));
        assert!(!is_media_track(TRACK_CLOCK_SYNC));
        assert!(!is_media_track(TRACK_SKIP));
        assert!(!is_media_track(TRACK_MOTION));
        assert!(!is_media_track(TRACK_SMART_EVENT));
        assert!(!is_media_track(TRACK_JPEG));
        assert!(!is_media_track(TRACK_TALKBACK));
        assert!(!is_media_track(TRACK_RESERVED));
    }

    #[test]
    fn test_track_info_payload_type_and_codec() {
        let info = track_info(TRACK_VIDEO).unwrap();
        assert_eq!(info.payload_type, 1);
        assert_eq!(info.codec.unwrap().tag, "VH264");
        assert_eq!(info.track_type, TrackType::VideoH264);

        let info = track_info(TRACK_VIDEO_HEVC).unwrap();
        assert_eq!(info.payload_type, 2);
        assert_eq!(info.codec.unwrap().tag, "VH265");

        let info = track_info(TRACK_AUDIO).unwrap();
        assert_eq!(info.payload_type, 4);
        assert_eq!(info.codec.unwrap().tag, "AAAC");

        let info = track_info(TRACK_AUDIO_OPUS).unwrap();
        assert_eq!(info.payload_type, 6);
        assert_eq!(info.codec.unwrap().tag, "AOPUS");
    }

    #[test]
    fn test_unknown_track_returns_none() {
        assert!(track_info(9999).is_none());
    }

    #[test]
    fn test_track_info_is_video_is_audio() {
        let video = track_info(TRACK_VIDEO).unwrap();
        assert!(video.is_video());
        assert!(!video.is_audio());

        let audio = track_info(TRACK_AUDIO).unwrap();
        assert!(!audio.is_video());
        assert!(audio.is_audio());

        let partition = track_info(TRACK_PARTITION).unwrap();
        assert!(!partition.is_video());
        assert!(!partition.is_audio());
    }
}
