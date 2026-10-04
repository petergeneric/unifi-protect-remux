//! Small deterministic fixtures shared by parser and consumer regressions.
pub fn append_record(bytes: &mut Vec<u8>, track: u16, untimed: bool, payload: &[u8]) -> usize {
    let start = bytes.len();
    let [hi, lo] = track.to_be_bytes();
    bytes.extend_from_slice(&[0xA0, hi, lo, 0xA0 ^ hi ^ lo]);
    bytes.extend_from_slice(if untimed {
        &[0xF1, 0x00, 0, 0]
    } else {
        &[0xF9, 0x0C, 0, 0]
    });
    if !untimed {
        bytes.extend_from_slice(&90000u32.to_be_bytes());
    }
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let back_size = (bytes.len() - start) as u32;
    bytes.extend_from_slice(&back_size.to_be_bytes());
    start
}

pub fn partition(bytes: &mut Vec<u8>) -> usize {
    append_record(bytes, 9, false, &[0; 20])
}
pub fn video(bytes: &mut Vec<u8>) -> usize {
    append_record(bytes, 7, false, b"PRIVATE_LAST_FRAME")
}

pub fn incomplete_video() -> (Vec<u8>, usize) {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    video(&mut bytes);
    let start = video(&mut bytes);
    let end = bytes.len();
    bytes[end - 4..].fill(0);
    bytes.resize(end + 32, 0);
    (bytes, start)
}

pub fn clock_sync(bytes: &mut Vec<u8>, rate: u32) -> usize {
    let start = bytes.len();
    bytes.extend_from_slice(&[0xA0, 0xDA, 0x7E, 0x04, 0xF9, 0x01, 0, 0]);
    bytes.extend_from_slice(&rate.to_be_bytes());
    bytes.extend_from_slice(&90000u32.to_be_bytes());
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&1700000000u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&28u32.to_be_bytes());
    start
}

pub fn zero_clock_rate() -> (Vec<u8>, usize) {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    video(&mut bytes);
    let bad = clock_sync(&mut bytes, 0);
    video(&mut bytes);
    (bytes, bad)
}
