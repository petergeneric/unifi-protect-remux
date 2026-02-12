use std::path::Path;

use clap::Parser;
use ubv::clock::wc_ticks_to_millis;
use ubv::partition::PartitionEntry;

#[derive(Parser)]
#[command(name = "ubv-info", about = "Parse and display UBV file structure")]
struct Args {
    /// Input .ubv file
    #[arg(short = 'f', long = "file")]
    file: Option<String>,

    /// Input .ubv file (positional)
    #[arg(conflicts_with = "file", required_unless_present_any = ["file", "schema", "version"])]
    input: Option<String>,

    /// Filter by track ID
    #[arg(short = 't', long = "track")]
    track_filter: Option<u16>,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Print JSON schema for the output format and exit
    #[arg(long)]
    schema: bool,

    /// Display version and quit
    #[arg(long)]
    version: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Reset SIGPIPE to default so piped output (e.g. head/tail) exits cleanly
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args = Args::parse();

    if args.version {
        print_version();
        return Ok(());
    }

    if args.schema {
        let schema = schemars::schema_for!(ubv::reader::UbvFile);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    let file = args.file.or(args.input).expect("file argument required");
    let mut reader = ubv::reader::open_ubv(Path::new(&file))?;
    let ubv = ubv::reader::parse_ubv(&mut reader)?;

    if args.json {
        println!("{}", serde_json::to_string(&ubv)?);
        return Ok(());
    }

    // Print header
    println!(
        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
        "Type", "TID", "KF", "OFFSET", "SIZE", "DTS", "CTS", "WC", "CR", ""
    );

    for partition in &ubv.partitions {
        println!("----------- PARTITION START -----------");

        let mut prev_wc_ms: Option<i64> = None;

        for entry in &partition.entries {
            match entry {
                PartitionEntry::ClockSync(cs) => {
                    println!(
                        "SC: {} ticks @{}Hz -> WC: {}ms",
                        cs.sc_dts, cs.sc_rate, cs.wc_ms
                    );
                }
                PartitionEntry::Frame(frame) => {
                    // Apply track filter if specified
                    if let Some(filter) = args.track_filter {
                        if frame.header.track_id != filter {
                            continue;
                        }
                    }

                    // Compute inter-frame wall-clock delta in milliseconds
                    let wc_ms = if frame.header.clock_rate > 0 {
                        wc_ticks_to_millis(frame.wc, frame.header.clock_rate) as i64
                    } else {
                        0
                    };
                    let delta_ms = match prev_wc_ms {
                        Some(prev) => wc_ms - prev,
                        None => 0,
                    };
                    prev_wc_ms = Some(wc_ms);

                    println!(
                        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
                        format!("{}", frame.type_char),
                        frame.header.track_id,
                        if frame.keyframe { 1 } else { 0 },
                        frame.header.data_offset,
                        frame.header.data_size,
                        frame.header.dts,
                        frame.cts,
                        frame.wc,
                        frame.header.clock_rate,
                        delta_ms,
                    );
                }
                PartitionEntry::Motion(m)
                | PartitionEntry::SmartEvent(m)
                | PartitionEntry::Jpeg(m)
                | PartitionEntry::Skip(m)
                | PartitionEntry::Talkback(m) => {
                    if let Some(filter) = args.track_filter {
                        if m.header.track_id != filter {
                            continue;
                        }
                    }

                    let type_char = match entry {
                        PartitionEntry::Motion(_) => "M",
                        PartitionEntry::SmartEvent(_) => "E",
                        PartitionEntry::Jpeg(_) => "J",
                        PartitionEntry::Skip(_) => "S",
                        PartitionEntry::Talkback(_) => "T",
                        _ => unreachable!(),
                    };

                    println!(
                        "{:>4} {:>5} {:>3} {:>16} {:>8} {:>15} {:>5} {:>17} {:>6} {:>7}",
                        type_char,
                        m.header.track_id,
                        "",
                        m.header.data_offset,
                        m.header.data_size,
                        m.header.dts,
                        "",
                        "",
                        m.header.clock_rate,
                        "",
                    );
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn print_version() {
    println!("UBV Info Tool");
    println!("Copyright (c) Peter Wright 2020-2026");
    println!("https://github.com/petergeneric/unifi-protect-remux");
    println!();

    let release = env!("RELEASE_VERSION");
    let commit = env!("GIT_COMMIT");
    if !release.is_empty() {
        println!("\tVersion:     {}", release);
    } else {
        println!("\tGit commit:  {}", commit);
    }
}
