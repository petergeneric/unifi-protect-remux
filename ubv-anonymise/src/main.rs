use clap::Parser;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
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
fn default_output_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    PathBuf::from(format!("anonymised-{stem}.ubv.gz"))
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.version {
        ubv::version::print_cli_version_banner(
            "UBV Anonymise Tool",
            env!("GIT_VERSION"),
            env!("GIT_COMMIT"),
        );
        return Ok(());
    }

    let input = args
        .input
        .ok_or("INPUT is required unless --version is specified")?;

    let gzip_output = args.output.is_none();
    let output = args.output.unwrap_or_else(|| default_output_path(&input));

    anonymise(&input, &output, gzip_output, args.keep_smart_events)
}

fn anonymise(
    input: &Path,
    output: &Path,
    gzip_output: bool,
    keep_smart_events: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if input.to_string_lossy().ends_with(".ubv.gz") {
        return Err(format!("'{}': .ubv.gz input is not supported for anonymisation; provide an uncompressed .ubv file", input.display()).into());
    }

    // Stage beside the destination so publication is an atomic rename. Parse
    // the private copy, so payload writes and validation use the same snapshot.
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut working = Some(NamedTempFile::new_in(parent)?);
    let mut compressed: Option<NamedTempFile> = None;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let stage = working.as_mut().unwrap();
        io::copy(&mut File::open(input)?, stage.as_file_mut())?;
        anonymise_staged_file(stage, keep_smart_events)?;
        if gzip_output {
            compressed = Some(NamedTempFile::new_in(parent)?);
            compress_staged_file(stage, compressed.as_mut().unwrap())?;
        }
        let slot = if gzip_output {
            &mut compressed
        } else {
            &mut working
        };
        match slot.take().unwrap().persist(output) {
            Ok(_) => Ok(()),
            Err(e) => {
                *slot = Some(e.file);
                Err(e.error.into())
            }
        }
    })();

    // Report cleanup failures accurately, including on the unsuccessful path.
    let mut cleanup_errors = Vec::new();
    for temp in [working, compressed].into_iter().flatten() {
        let path = temp.path().display().to_string();
        if let Err(e) = temp.close() {
            cleanup_errors.push(format!("removing temporary file '{path}': {e}"));
        }
    }
    if !cleanup_errors.is_empty() {
        return Err(format!(
            "{}; {}",
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "output published".into()),
            cleanup_errors.join("; ")
        )
        .into());
    }
    result?;
    log::info!("Done, wrote {}", output.display());
    Ok(())
}

fn anonymise_staged_file(
    stage: &mut NamedTempFile,
    keep_smart_events: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = open_ubv(stage.path())?;
    let mut records = record::RecordReader::new(&mut reader)?;
    let out = stage.as_file_mut();
    log::info!("Anonymising records...");
    let mut records_zeroed: u64 = 0;
    let mut bytes_zeroed: u64 = 0;
    let mut record_count: u64 = 0;

    while let record::ReadOutcome::Record(rec) = records.read_next()? {
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
                    && !keep_smart_events =>
            {
                true
            }
            _ if rec.track_id == track::TRACK_PARTITION && rec.data_size > 32 => true,
            _ => false,
        };

        if should_zero && rec.data_size > 0 {
            zero_region(out, rec.data_offset, rec.data_size).map_err(|e| {
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

    out.sync_all()?;
    Ok(())
}

fn compress_staged_file(source: &NamedTempFile, target: &mut NamedTempFile) -> io::Result<()> {
    let mut encoder = GzEncoder::new(BufWriter::new(target.as_file_mut()), Compression::default());
    io::copy(
        &mut BufReader::new(File::open(source.path())?),
        &mut encoder,
    )?;
    encoder.finish()?.flush()?;
    target.as_file().sync_all()
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

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../testdata/support/records.rs"]
mod fixtures;

#[cfg(test)]
mod tests {
    use super::fixtures;
    use super::*;

    fn args(input: &Path, output: &Path) -> Args {
        Args {
            version: false,
            keep_smart_events: false,
            input: Some(input.into()),
            output: Some(output.into()),
        }
    }

    #[test]
    fn malformed_input_never_publishes_or_overwrites_output_and_cleans_stage() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.ubv");
        let output = dir.path().join("output.ubv");
        let (bytes, _) = fixtures::incomplete_video();
        std::fs::write(&input, &bytes).unwrap();
        assert!(run(args(&input, &output)).is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        std::fs::write(&output, b"existing output").unwrap();
        assert!(run(args(&input, &output)).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn complete_output_zeroes_all_frames_after_index_and_skip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.ubv");
        let output = dir.path().join("output.ubv");
        let mut bytes = Vec::new();
        fixtures::partition(&mut bytes);
        fixtures::video(&mut bytes);
        fixtures::append_record(&mut bytes, 10, true, &[3; 28]);
        fixtures::append_record(&mut bytes, 6, true, &[0; 2048]);
        fixtures::partition(&mut bytes);
        fixtures::video(&mut bytes);
        std::fs::write(&input, &bytes).unwrap();
        run(args(&input, &output)).unwrap();
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
        let anonymised = std::fs::read(&output).unwrap();
        assert_eq!(anonymised.len(), bytes.len());
        let file = ubv::reader::parse_ubv(&mut std::io::Cursor::new(&anonymised)).unwrap();
        assert!(file.read_status.is_complete());
        assert_eq!(file.partitions.len(), 2);
        for partition in file.partitions {
            for entry in partition.entries {
                if let ubv::partition::PartitionEntry::Frame(frame) = entry {
                    let start = frame.header.data_offset as usize;
                    assert!(
                        anonymised[start..start + frame.header.data_size as usize]
                            .iter()
                            .all(|&b| b == 0)
                    );
                }
            }
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    #[test]
    fn gzip_output_is_finished_and_only_published_after_validation() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.ubv");
        let output = dir.path().join("output.ubv.gz");
        let (bad, _) = fixtures::incomplete_video();
        std::fs::write(&input, bad).unwrap();
        assert!(anonymise(&input, &output, true, false).is_err());
        assert!(!output.exists());
        let mut bytes = Vec::new();
        fixtures::partition(&mut bytes);
        fixtures::video(&mut bytes);
        std::fs::write(&input, bytes).unwrap();
        anonymise(&input, &output, true, false).unwrap();
        let mut reader = open_ubv(&output).unwrap();
        let file = ubv::reader::parse_ubv(&mut reader).unwrap();
        assert!(file.read_status.is_complete());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    #[test]
    fn publication_failure_preserves_destination_and_cleans_both_stages() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.ubv");
        let output = dir.path().join("destination");
        std::fs::create_dir(&output).unwrap();
        let marker = output.join("existing");
        std::fs::write(&marker, b"preserve").unwrap();
        let mut bytes = Vec::new();
        fixtures::partition(&mut bytes);
        fixtures::video(&mut bytes);
        std::fs::write(&input, &bytes).unwrap();
        for gzip_output in [false, true] {
            assert!(anonymise(&input, &output, gzip_output, false).is_err());
            assert_eq!(std::fs::read(&marker).unwrap(), b"preserve");
            assert_eq!(std::fs::read(&input).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        }
    }
}
