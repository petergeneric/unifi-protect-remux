use std::ffi::{c_int, c_void, CString};
use std::io;
use std::ptr;

extern crate ffmpeg_next as ffmpeg;

use crate::demux;
use ubv::frame::RecordHeader;
use ubv::track::is_video_track;

const AVIO_BUF_SIZE: usize = 4096;

/// AVERROR_EOF: -(MKTAG('E','O','F',' '))
const AVERROR_EOF: c_int = -(0x45 | (0x4F << 8) | (0x46 << 16) | (0x20 << 24));

/// State passed to the AVIO read callback via the opaque pointer.
struct ReadState {
    data: Vec<u8>,
    pos: usize,
}

/// AVIO read callback: reads from the in-memory buffer.
unsafe extern "C" fn avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    unsafe {
        let state = &mut *(opaque as *mut ReadState);
        if buf_size <= 0 {
            return AVERROR_EOF;
        }
        let remaining = state.data.len() - state.pos;
        if remaining == 0 {
            return AVERROR_EOF;
        }
        let to_read = (buf_size as usize).min(remaining);
        ptr::copy_nonoverlapping(state.data.as_ptr().add(state.pos), buf, to_read);
        state.pos += to_read;
        to_read as c_int
    }
}

/// Map a UBV track ID to the FFmpeg raw format short name for probing.
fn format_name_for_track(track_id: u16) -> Option<&'static str> {
    use ubv::track::*;
    match track_id {
        TRACK_VIDEO => Some("h264"),
        TRACK_VIDEO_HEVC => Some("hevc"),
        TRACK_VIDEO_AV1 => Some("av1"),
        TRACK_AUDIO => Some("aac"),
        TRACK_AUDIO_OPUS => Some("ogg"),
        TRACK_AUDIO_RAW => Some("alaw"),
        _ => None,
    }
}

/// Probe codec parameters from a UBV file's frames using FFmpeg's custom AVIO API.
///
/// Demuxes the first few frames into an in-memory buffer, then uses FFmpeg to detect
/// the stream and extract codec parameters (SPS/PPS, sample rate, channels, etc.).
pub fn probe_stream_params(
    ubv_path: &str,
    frames: &[RecordHeader],
    track_id: u16,
) -> io::Result<ffmpeg::codec::Parameters> {
    if frames.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No frames to probe",
        ));
    }

    let format_name = format_name_for_track(track_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsupported track ID for probing: {}", track_id),
        )
    })?;

    // Demux first ~10 frames into memory for probing
    let probe_count = frames.len().min(10);
    let probe_frames = &frames[..probe_count];
    let mut data = Vec::new();
    if is_video_track(track_id) {
        demux::demux_video_frames(ubv_path, probe_frames, &mut data)?;
    } else {
        demux::demux_audio_frames(ubv_path, probe_frames, &mut data)?;
    }

    if data.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Probed frames produced no data",
        ));
    }

    unsafe { probe_from_buffer(data, format_name) }
}

/// Probe codec parameters from an in-memory buffer using FFmpeg AVIO.
unsafe fn probe_from_buffer(
    data: Vec<u8>,
    format_name: &str,
) -> io::Result<ffmpeg::codec::Parameters> {
    use ffmpeg_sys_next::*;

    // State for the read callback — must outlive all FFmpeg operations
    let mut state = Box::new(ReadState { data, pos: 0 });
    let state_ptr: *mut c_void = &mut *state as *mut ReadState as *mut c_void;

    // Allocate AVIO internal buffer (FFmpeg may reallocate it)
    let avio_buf = unsafe { av_malloc(AVIO_BUF_SIZE) } as *mut u8;
    if avio_buf.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "av_malloc failed for AVIO buffer",
        ));
    }

    // Create AVIO context (read-only, no write, no seek)
    let mut avio_ctx = unsafe {
        avio_alloc_context(
            avio_buf,
            AVIO_BUF_SIZE as c_int,
            0, // read-only
            state_ptr,
            Some(avio_read_callback),
            None,
            None,
        )
    };
    if avio_ctx.is_null() {
        unsafe {
            av_free(avio_buf as *mut c_void);
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "avio_alloc_context failed",
        ));
    }

    // Probe using the AVIO context; cleanup AVIO afterwards regardless of result
    let result = unsafe { do_probe(avio_ctx, format_name) };

    // Cleanup: free AVIO buffer (may have been reallocated internally) then the context
    unsafe {
        av_freep(ptr::addr_of_mut!((*avio_ctx).buffer) as *mut c_void);
        avio_context_free(&mut avio_ctx);
    }
    drop(state);

    result
}

/// Inner probing logic, separated so the caller can handle AVIO cleanup uniformly.
unsafe fn do_probe(
    avio_ctx: *mut ffmpeg_sys_next::AVIOContext,
    format_name: &str,
) -> io::Result<ffmpeg::codec::Parameters> {
    use ffmpeg_sys_next::*;

    let fmt_cstr = CString::new(format_name).unwrap();
    let input_fmt = unsafe { av_find_input_format(fmt_cstr.as_ptr()) };
    if input_fmt.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unknown format '{}' for probing", format_name),
        ));
    }

    // Create format context with our custom AVIO
    let mut fmt_ctx = unsafe { avformat_alloc_context() };
    if fmt_ctx.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "avformat_alloc_context failed",
        ));
    }
    unsafe {
        (*fmt_ctx).pb = avio_ctx;
    }

    // Open input — on failure, avformat_open_input frees fmt_ctx
    let ret =
        unsafe { avformat_open_input(&mut fmt_ctx, ptr::null(), input_fmt, ptr::null_mut()) };
    if ret < 0 {
        // fmt_ctx is already freed by avformat_open_input on error
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("avformat_open_input failed (error {})", ret),
        ));
    }

    // Probe stream info
    let ret = unsafe { avformat_find_stream_info(fmt_ctx, ptr::null_mut()) };
    if ret < 0 {
        unsafe { avformat_close_input(&mut fmt_ctx) };
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("avformat_find_stream_info failed (error {})", ret),
        ));
    }

    if unsafe { (*fmt_ctx).nb_streams } == 0 {
        unsafe { avformat_close_input(&mut fmt_ctx) };
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No streams found during probing",
        ));
    }

    // Copy codec parameters from the first probed stream
    let stream = unsafe { *(*fmt_ctx).streams };
    let src_params = unsafe { (*stream).codecpar };
    let mut params = ffmpeg::codec::Parameters::new();
    let ret = unsafe { avcodec_parameters_copy(params.as_mut_ptr(), src_params) };
    unsafe { avformat_close_input(&mut fmt_ctx) };

    if ret < 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("avcodec_parameters_copy failed (error {})", ret),
        ));
    }

    // Log probed parameters for diagnostics
    let p = unsafe { &*params.as_ptr() };
    let extradata_size = p.extradata_size;
    let extradata_fmt = if extradata_size > 0 && !p.extradata.is_null() {
        let first_byte = unsafe { *p.extradata };
        if first_byte == 0x01 { "AVCC" } else { "Annex B" }
    } else {
        "EMPTY"
    };
    log::info!(
        "Probed: codec_id={:?}, {}x{}, extradata={}B ({})",
        p.codec_id,
        p.width,
        p.height,
        extradata_size,
        extradata_fmt,
    );

    Ok(params)
}
