extern crate ffmpeg_next as ffmpeg;
extern crate ffmpeg_sys_next as ffi;

use std::io;

/// Extract a JPEG thumbnail from the first video frame of a media file.
///
/// The thumbnail is scaled so its width does not exceed `max_width` pixels
/// (preserving the aspect ratio) and saved as JPEG to `output_path`.
pub fn extract_thumbnail(input_path: &str, output_path: &str, max_width: u32) -> io::Result<()> {
    let mut ictx = ffmpeg::format::input(&input_path)
        .map_err(|e| io_err(format!("Opening {}: {}", input_path, e)))?;

    let video = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| io_err_s("No video stream"))?;
    let stream_idx = video.index();

    let ctx = ffmpeg::codec::context::Context::from_parameters(video.parameters())
        .map_err(|e| io_err(format!("Decoder context: {}", e)))?;
    let mut decoder = ctx
        .decoder()
        .video()
        .map_err(|e| io_err(format!("Video decoder: {}", e)))?;

    let (src_w, src_h) = (decoder.width(), decoder.height());
    if src_w == 0 || src_h == 0 {
        return Err(io_err_s("Zero video dimensions"));
    }

    let (dst_w, dst_h) = scaled_dims(src_w, src_h, max_width);

    let mut scaler = ffmpeg::software::scaling::Context::get(
        decoder.format(),
        src_w,
        src_h,
        ffmpeg::format::Pixel::YUVJ420P,
        dst_w,
        dst_h,
        ffmpeg::software::scaling::Flags::BILINEAR,
    )
    .map_err(|e| io_err(format!("Scaler: {}", e)))?;

    let mut decoded = ffmpeg::frame::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_idx {
            continue;
        }
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if decoder.receive_frame(&mut decoded).is_ok() {
            return scale_and_encode(&mut scaler, &mut decoded, dst_w, dst_h, output_path);
        }
    }

    // Flush the decoder to retrieve any buffered frames (e.g. H.264/HEVC
    // with B-frames may buffer the first packets before producing output).
    decoder.send_eof().ok();
    if decoder.receive_frame(&mut decoded).is_ok() {
        return scale_and_encode(&mut scaler, &mut decoded, dst_w, dst_h, output_path);
    }

    Err(io_err_s("No video frames decoded"))
}

/// Scale a decoded frame, encode as JPEG, and write to disk.
fn scale_and_encode(
    scaler: &mut ffmpeg::software::scaling::Context,
    decoded: &mut ffmpeg::frame::Video,
    dst_w: u32,
    dst_h: u32,
    output_path: &str,
) -> io::Result<()> {
    let mut scaled = ffmpeg::frame::Video::empty();
    scaler
        .run(decoded, &mut scaled)
        .map_err(|e| io_err(format!("Scaling: {}", e)))?;

    let jpeg = unsafe { encode_mjpeg(&mut scaled, dst_w, dst_h)? };
    std::fs::write(output_path, &jpeg)?;
    Ok(())
}

/// Compute scaled dimensions preserving aspect ratio, clamped to even values.
fn scaled_dims(src_w: u32, src_h: u32, max_w: u32) -> (u32, u32) {
    if src_w <= max_w {
        return (src_w & !1, src_h & !1);
    }
    let r = max_w as f64 / src_w as f64;
    let w = ((src_w as f64 * r).round() as u32).max(2) & !1;
    let h = ((src_h as f64 * r).round() as u32).max(2) & !1;
    (w, h)
}

/// Encode a YUVJ420P frame as JPEG using FFmpeg's MJPEG encoder (raw API).
///
/// # Safety
///
/// Operates on raw FFmpeg pointers; the caller must ensure `frame` contains
/// valid YUVJ420P pixel data at the given dimensions.
unsafe fn encode_mjpeg(
    frame: &mut ffmpeg::frame::Video,
    width: u32,
    height: u32,
) -> io::Result<Vec<u8>> {
    let codec = unsafe { ffi::avcodec_find_encoder(ffi::AVCodecID::AV_CODEC_ID_MJPEG) };
    if codec.is_null() {
        return Err(io_err_s("MJPEG encoder not found"));
    }

    let mut ctx = unsafe { ffi::avcodec_alloc_context3(codec) };
    if ctx.is_null() {
        return Err(io_err_s("avcodec_alloc_context3 failed"));
    }

    unsafe {
        (*ctx).width = width as i32;
        (*ctx).height = height as i32;
        (*ctx).pix_fmt = ffi::AVPixelFormat::AV_PIX_FMT_YUVJ420P;
        (*ctx).time_base = ffi::AVRational { num: 1, den: 1 };
    }

    let ret = unsafe { ffi::avcodec_open2(ctx, codec, std::ptr::null_mut()) };
    if ret < 0 {
        unsafe { ffi::avcodec_free_context(&mut ctx) };
        return Err(io_err(format!("avcodec_open2: AVERROR {}", ret)));
    }

    let frame_ptr = unsafe { frame.as_mut_ptr() };
    unsafe {
        (*frame_ptr).pts = 0;
    }

    let ret = unsafe { ffi::avcodec_send_frame(ctx, frame_ptr) };
    if ret < 0 {
        unsafe { ffi::avcodec_free_context(&mut ctx) };
        return Err(io_err(format!("avcodec_send_frame: AVERROR {}", ret)));
    }

    let mut pkt = unsafe { ffi::av_packet_alloc() };
    if pkt.is_null() {
        unsafe { ffi::avcodec_free_context(&mut ctx) };
        return Err(io_err_s("av_packet_alloc failed"));
    }

    let ret = unsafe { ffi::avcodec_receive_packet(ctx, pkt) };
    if ret < 0 {
        unsafe {
            ffi::av_packet_free(&mut pkt);
            ffi::avcodec_free_context(&mut ctx);
        }
        return Err(io_err(format!("avcodec_receive_packet: AVERROR {}", ret)));
    }

    let data = unsafe { std::slice::from_raw_parts((*pkt).data, (*pkt).size as usize) };
    let result = data.to_vec();

    unsafe {
        ffi::av_packet_free(&mut pkt);
        ffi::avcodec_free_context(&mut ctx);
    }

    Ok(result)
}

fn io_err(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}

fn io_err_s(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}
