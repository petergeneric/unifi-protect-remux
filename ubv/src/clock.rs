use crate::error::{Result, UbvError};

/// Clock sync data parsed from a clock sync record (track 0xDA7E).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct ClockSync {
    /// Stream clock DTS value from the clock sync record header.
    pub sc_dts: u64,
    /// Stream clock rate (always 1000 Hz = milliseconds).
    pub sc_rate: u32,
    /// Wall-clock time in milliseconds since epoch.
    pub wc_ms: u64,
    /// Raw wall-clock seconds from the payload.
    pub wc_seconds: u32,
    /// Raw wall-clock nanoseconds from the payload.
    pub wc_nanoseconds: u32,
}

/// Convert a wall-clock value (in track clock_rate units) to milliseconds since epoch.
///
/// Uses round-half-up to match the rounding convention used by `compute_wall_clock`.
/// Uses u128 arithmetic internally to avoid overflow with large wall-clock values.
pub fn wc_ticks_to_millis(wc: u64, clock_rate: u32) -> u64 {
    ((wc as u128 * 1000 + clock_rate as u128 / 2) / clock_rate as u128) as u64
}

impl ClockSync {
    /// Parse a clock sync from the record's DTS, clock rate, and 8-byte payload.
    pub fn from_record(dts: u64, clock_rate: u32, payload: &[u8]) -> Result<Self> {
        if payload.len() < 8 {
            return Err(UbvError::ShortPayload {
                expected: 8,
                got: payload.len(),
            });
        }
        let wc_seconds = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let wc_nanoseconds = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let wc_ms = wc_seconds as u64 * 1000 + wc_nanoseconds as u64 / 1_000_000;

        Ok(ClockSync {
            sc_dts: dts,
            sc_rate: clock_rate,
            wc_ms,
            wc_seconds,
            wc_nanoseconds,
        })
    }

    /// Compute wall-clock value for a frame, expressed in the frame's clock rate units.
    ///
    /// The firmware's `ComputeWallClock` converts WC and SC separately to the
    /// target clock rate (with integer round-half-up), then computes the offset:
    ///
    ///   wc_ticks = round(wc_ms * frame_rate / sc_rate)
    ///   sc_ticks = round(sc_dts * frame_rate / sc_rate)
    ///   frame_wc = frame_dts + wc_ticks - sc_ticks
    ///
    /// Uses i128 to avoid overflow.
    pub fn compute_wall_clock(&self, frame_dts: u64, frame_rate: u32) -> u64 {
        let fr = frame_rate as i128;
        let sr = self.sc_rate as i128;
        let half = sr / 2;

        let wc_ticks = (self.wc_ms as i128 * fr + half) / sr;
        let sc_ticks = (self.sc_dts as i128 * fr + half) / sr;

        (frame_dts as i128 + wc_ticks - sc_ticks).max(0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_sync_from_record_old_file() {
        let payload = [0x64, 0x5d, 0xc6, 0x12, 0x34, 0xed, 0xce, 0x00];
        let cs = ClockSync::from_record(1139129710, 1000, &payload).unwrap();
        assert_eq!(cs.sc_dts, 1139129710);
        assert_eq!(cs.sc_rate, 1000);
        assert_eq!(cs.wc_ms, 1683867154888);
    }

    #[test]
    fn test_compute_wall_clock_video_old_file() {
        let cs = ClockSync {
            sc_dts: 1139129710,
            sc_rate: 1000,
            wc_ms: 1683867154888,
            wc_seconds: 1683867154,
            wc_nanoseconds: 888000000,
        };
        let wc = cs.compute_wall_clock(102521673899, 90000);
        assert_eq!(wc, 151548043939919);
    }

    #[test]
    fn test_compute_wall_clock_audio_old_file() {
        let cs = ClockSync {
            sc_dts: 1139129710,
            sc_rate: 1000,
            wc_ms: 1683867154888,
            wc_seconds: 1683867154,
            wc_nanoseconds: 888000000,
        };
        let wc = cs.compute_wall_clock(50235621221, 44100);
        assert_eq!(wc, 74258541531571);

        let wc = cs.compute_wall_clock(50235622245, 44100);
        assert_eq!(wc, 74258541532595);
    }

    #[test]
    fn test_compute_wall_clock_new_file() {
        let payload = [0x69, 0x8b, 0xcc, 0x91, 0x20, 0xc8, 0x55, 0x80];
        let cs = ClockSync::from_record(8578090739, 1000, &payload).unwrap();
        assert_eq!(cs.wc_ms, 1770769553550);

        let wc = cs.compute_wall_clock(772028166536, 90000);
        assert_eq!(wc, 159369259819526);

        let wc = cs.compute_wall_clock(137249449847, 16000);
        assert_eq!(wc, 28332312854823);
    }

    #[test]
    fn test_clock_sync_from_record_short_payload() {
        let payload = [0x64, 0x5d, 0xc6];
        let result = ClockSync::from_record(0, 1000, &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_wall_clock_saturates_at_zero() {
        // frame_dts before the clock sync reference point → result would be negative
        let cs = ClockSync {
            sc_dts: 1000,
            sc_rate: 1000,
            wc_ms: 0,
            wc_seconds: 0,
            wc_nanoseconds: 0,
        };
        // wc_ticks = 0, sc_ticks = 90000, frame_dts as i128 + 0 - 90000 = -90000 + 100 = -89900
        let wc = cs.compute_wall_clock(100, 90000);
        assert_eq!(wc, 0);
    }

    #[test]
    fn test_compute_wall_clock_problematic_sc() {
        // SC that previously caused ±1 errors with combined formula
        let cs = ClockSync {
            sc_dts: 1139808043,
            sc_rate: 1000,
            wc_ms: 1683867833226,
            wc_seconds: 1683867833,
            wc_nanoseconds: 226000000,
        };
        let wc = cs.compute_wall_clock(50265538405, 44100);
        assert_eq!(wc, 74258571448976);
    }

    #[test]
    fn test_wc_ticks_to_millis_identity_at_1000hz() {
        // At 1000 Hz, ticks ARE milliseconds
        assert_eq!(wc_ticks_to_millis(1683867154888, 1000), 1683867154888);
    }

    #[test]
    fn test_wc_ticks_to_millis_90khz() {
        // 151548043939920 ticks at 90kHz → 1683867154888 ms (from old file test data)
        // Exact: 151548043939920 * 1000 / 90000 = 1683867154888.0
        assert_eq!(wc_ticks_to_millis(151548043939920, 90000), 1683867154888);
    }

    #[test]
    fn test_wc_ticks_to_millis_rounds_half_up() {
        // 45 ticks at 90000 Hz → 0.5 ms, should round up to 1
        assert_eq!(wc_ticks_to_millis(45, 90000), 1);
        // 44 ticks at 90000 Hz → 0.4889 ms, should truncate to 0
        assert_eq!(wc_ticks_to_millis(44, 90000), 0);
    }
}
