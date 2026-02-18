use clap::Parser;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read as _, Seek, SeekFrom, Write};
use std::path::PathBuf;
use ubv::reader::open_ubv;
use ubv::record;
use ubv::track;

#[derive(Parser)]
#[command(about = "Strip audio/video/image essence from a .ubv file, preserving record structure")]
struct Args {
    /// Display version and quit
    #[arg(long)]
    version: bool,

    /// Keep SmartEvent metadata (do not anonymise)
    #[arg(long)]
    keep_smart_events: bool,

    /// Input .ubv file
    input: Option<PathBuf>,
    /// Output .ubv file (anonymised copy); if omitted, writes anonymised-<name>.ubv.gz in the current directory
    output: Option<PathBuf>,
}

/// Zero out a region of the file at the given offset and size.
fn zero_region(file: &mut File, offset: u64, size: u32) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    let buf = [0u8; 65536];
    let mut remaining = size as usize;
    while remaining > 0 {
        let chunk = remaining.min(buf.len());
        file.write_all(&buf[..chunk])?;
        remaining -= chunk;
    }
    Ok(())
}

/// Derive a default output path: anonymised-<stem>.ubv.gz in the current directory.
fn default_output_path(input: &PathBuf) -> PathBuf {
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    PathBuf::from(format!("anonymised-{stem}.ubv.gz"))
}

/// Gzip compress `src` to `dst`, then remove `src`.
fn gzip_and_cleanup(src: &PathBuf, dst: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let src_size = fs::metadata(src)
        .map(|m| m.len())
        .unwrap_or(0);
    log::info!(
        "Compressing {} ({:.1} MB) -> {}...",
        src.display(),
        src_size as f64 / (1024.0 * 1024.0),
        dst.display()
    );

    let input = File::open(src)
        .map_err(|e| format!("Opening '{}' for gzip compression: {}", src.display(), e))?;
    let mut reader = BufReader::new(input);

    let output = File::create(dst)
        .map_err(|e| format!("Creating '{}': {}", dst.display(), e))?;
    let mut encoder = GzEncoder::new(BufWriter::new(output), Compression::default());

    let mut buf = [0u8; 65536];
    let mut bytes_compressed: u64 = 0;
    let mut last_log_mb: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        encoder.write_all(&buf[..n])?;
        bytes_compressed += n as u64;

        let current_mb = bytes_compressed / (100 * 1024 * 1024);
        if current_mb > last_log_mb {
            log::info!(
                "Compressing: {:.0} MB processed...",
                bytes_compressed as f64 / (1024.0 * 1024.0)
            );
            last_log_mb = current_mb;
        }
    }
    encoder.finish()?;

    let dst_size = fs::metadata(dst)
        .map(|m| m.len())
        .unwrap_or(0);
    log::info!(
        "Compressed {:.1} MB -> {:.1} MB",
        src_size as f64 / (1024.0 * 1024.0),
        dst_size as f64 / (1024.0 * 1024.0)
    );

    fs::remove_file(src)
        .map_err(|e| format!("Removing temp file '{}': {}", src.display(), e))?;

    Ok(())
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.version {
        ubv::version::print_cli_version_banner(
            "UBV Anonymise Tool",
            env!("CARGO_PKG_VERSION"),
            env!("RELEASE_VERSION"),
            env!("GIT_COMMIT"),
        );
        return Ok(());
    }

    let input = args.input.ok_or("INPUT is required unless --version is specified")?;

    let gzip_output = args.output.is_none();
    let output = args.output.unwrap_or_else(|| default_output_path(&input));

    // When gzipping, anonymise to a temp .ubv file first, then compress
    let working_file = if gzip_output {
        output.with_extension("")
    } else {
        output.clone()
    };

    if input.to_string_lossy().ends_with(".ubv.gz") {
        return Err(format!(
            "'{}': .ubv.gz input is not supported for anonymisation; provide an uncompressed .ubv file",
            input.display()
        ).into());
    }

    log::info!("Copying {} -> {}...", input.display(), working_file.display());
    fs::copy(&input, &working_file).map_err(|e| {
        format!("Copying '{}' to '{}': {}", input.display(), working_file.display(), e)
    })?;

    let mut reader = open_ubv(&input).map_err(|e| {
        format!("Opening input '{}': {}", input.display(), e)
    })?;
    let mut out = OpenOptions::new().write(true).open(&working_file).map_err(|e| {
        format!("Opening output '{}' for writing: {}", working_file.display(), e)
    })?;

    log::info!("Anonymising records...");
    let mut records_zeroed: u64 = 0;
    let mut bytes_zeroed: u64 = 0;
    let mut record_count: u64 = 0;

    loop {
        let rec = match record::read_record(&mut reader) {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(e) => {
                eprintln!(
                    "Warning: record parse error after {} records: {e}",
                    record_count
                );
                break;
            }
        };
        record_count += 1;

        let should_zero = match track::track_info(rec.track_id) {
            Some(info) if info.is_video() || info.is_audio() => true,
            Some(info)
                if matches!(
                    info.track_type,
                    track::TrackType::Jpeg | track::TrackType::Talkback
                ) =>
            {
                true
            }
            Some(info)
                if matches!(info.track_type, track::TrackType::SmartEvent)
                    && !args.keep_smart_events =>
            {
                true
            }
            _ if rec.track_id == track::TRACK_PARTITION && rec.data_size > 32 => true,
            _ => false,
        };

        if should_zero && rec.data_size > 0 {
            zero_region(&mut out, rec.data_offset, rec.data_size).map_err(|e| {
                format!(
                    "Zeroing record #{} (track=0x{:04X}, offset=0x{:X}, size={}): {}",
                    record_count, rec.track_id, rec.data_offset, rec.data_size, e
                )
            })?;
            records_zeroed += 1;
            bytes_zeroed += rec.data_size as u64;
        }
    }

    log::info!(
        "Anonymised {} records, zeroed {} bytes ({:.1} MB)",
        records_zeroed,
        bytes_zeroed,
        bytes_zeroed as f64 / (1024.0 * 1024.0)
    );

    // Close the output file before gzip reads it
    drop(out);

    if gzip_output {
        gzip_and_cleanup(&working_file, &output)?;
    }

    log::info!("Done, wrote {}", output.display());

    Ok(())
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let args = Args::parse();
    if let Err(e) = run(args) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
