//! Synthetic record builder for unit tests (no real recordings involved).

/// Accumulates records into an in-memory `.ubv` byte stream, tracking the
/// absolute offset so alignment padding matches the parser's.
#[derive(Default)]
pub struct UbvBuilder {
    pub bytes: Vec<u8>,
}

fn tag(track_id: u16) -> [u8; 4] {
    let hi = (track_id >> 8) as u8;
    let lo = track_id as u8;
    [0xA0, hi, lo, 0xA0 ^ hi ^ lo]
}

fn pad_for(len: u64) -> usize {
    ((4 - (len % 4)) % 4) as usize
}

impl UbvBuilder {
    pub fn offset(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Timed record with bit 6 of byte 4 set (duration doubles as SIZE), no
    /// extra field and a table clock rate (SRI != 1). DTS is 64-bit when bit 2
    /// of byte 4 is set, 32-bit otherwise.
    pub fn timed(
        &mut self,
        track_id: u16,
        b4: u8,
        b5: u8,
        seq: u16,
        dts: u64,
        payload: &[u8],
    ) -> u64 {
        assert!(b4 & 0x40 != 0 && b4 & 0x02 == 0 && b5 & 0x0F != 1);
        let start = self.offset();
        self.bytes.extend_from_slice(&tag(track_id));
        self.bytes.extend_from_slice(&[b4, b5]);
        self.bytes.extend_from_slice(&seq.to_be_bytes());
        if b4 & 0x04 != 0 {
            self.bytes.extend_from_slice(&dts.to_be_bytes());
        } else {
            self.bytes.extend_from_slice(&(dts as u32).to_be_bytes());
        }
        self.finish_record(start, payload)
    }

    /// Untimed record: format `F1 00`, no DTS, SIZE at bytes 8-11.
    pub fn untimed(&mut self, track_id: u16, seq: u16, payload: &[u8]) -> u64 {
        let start = self.offset();
        self.bytes.extend_from_slice(&tag(track_id));
        self.bytes.extend_from_slice(&[0xF1, 0x00]);
        self.bytes.extend_from_slice(&seq.to_be_bytes());
        self.finish_record(start, payload)
    }

    /// SIZE + DATA + PAD + BACK_SIZE. Returns the record's start offset.
    fn finish_record(&mut self, start: u64, payload: &[u8]) -> u64 {
        self.bytes
            .extend_from_slice(&(payload.len() as u32).to_be_bytes());
        self.bytes.extend_from_slice(payload);
        let pad = pad_for(self.offset());
        self.bytes.extend(std::iter::repeat_n(0u8, pad));
        let back = (self.offset() - start) as u32;
        self.bytes.extend_from_slice(&back.to_be_bytes());
        start
    }

    /// Partition header (track 9, `FD 0C`, 64-bit DTS) with a 20-byte payload.
    pub fn partition_header(&mut self, dts: u64) -> u64 {
        self.timed(9, 0xFD, 0x0C, 0, dts, &[0x11; 20])
    }

    /// Clock sync (track 0xDA7E, `F9 02`, 1 kHz): `dts_ms` maps to `wc_secs`.
    pub fn clock_sync(&mut self, dts_ms: u64, wc_secs: u32) -> u64 {
        let mut payload = wc_secs.to_be_bytes().to_vec();
        payload.extend_from_slice(&0u32.to_be_bytes());
        self.timed(0xDA7E, 0xF9, 0x02, 0, dts_ms, &payload)
    }

    /// H.264 video frame (track 7, 90 kHz, 64-bit DTS).
    pub fn video(&mut self, seq: u16, dts: u64, keyframe: bool, len: usize) -> u64 {
        let b4 = if keyframe { 0xED } else { 0xCD };
        self.timed(7, b4, 0x0C, seq, dts, &vec![0x42; len])
    }
}

/// Payload shaped like the per-partition index seen on track 10: a short
/// header starting with zero bytes, then (ms, offset) pairs.
pub fn index_payload(pairs: &[(u64, u64)]) -> Vec<u8> {
    let mut p = vec![0u8; 8];
    for (ms, off) in pairs {
        p.extend_from_slice(&ms.to_be_bytes());
        p.extend_from_slice(&off.to_be_bytes());
    }
    p
}
