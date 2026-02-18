use std::io::Write;
use std::sync::mpsc;

use flate2::write::GzEncoder;
use flate2::Compression;
use remux_lib::{ProgressEvent, RemuxConfig};

pub enum WorkerMessage {
    Progress { file_index: usize, event: ProgressEvent },
    Done,
}

pub enum DiagnosticsMessage {
    FileStarted { file_index: usize },
    FileCompleted { file_index: usize, output_path: String },
    FileFailed { file_index: usize, error: String },
    Done,
}

pub fn spawn_diagnostics(
    files: Vec<String>,
    tx: mpsc::Sender<DiagnosticsMessage>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for (file_index, path) in files.iter().enumerate() {
            let _ = tx.send(DiagnosticsMessage::FileStarted { file_index });

            match produce_diagnostics(path) {
                Ok(output_path) => {
                    let _ = tx.send(DiagnosticsMessage::FileCompleted {
                        file_index,
                        output_path,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DiagnosticsMessage::FileFailed {
                        file_index,
                        error: e.to_string(),
                    });
                }
            }
        }
        let _ = tx.send(DiagnosticsMessage::Done);
    })
}

fn produce_diagnostics(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let ubv_path = std::path::Path::new(path);
    let mut reader = ubv::reader::open_ubv(ubv_path)?;
    let ubv_file = ubv::reader::parse_ubv(&mut reader)?;

    let json = serde_json::to_string(&ubv_file)?;

    let output_path = format!("{}.json.gz", path);
    let out_file = std::fs::File::create(&output_path)?;
    let mut encoder = GzEncoder::new(out_file, Compression::default());
    encoder.write_all(json.as_bytes())?;
    encoder.finish()?;

    Ok(output_path)
}

pub fn spawn(
    files: Vec<String>,
    config: RemuxConfig,
    tx: mpsc::Sender<WorkerMessage>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for (file_index, path) in files.iter().enumerate() {
            let _ = remux_lib::process_file(path, &config, &mut |event| {
                let _ = tx.send(WorkerMessage::Progress {
                    file_index,
                    event,
                });
            });
        }
        let _ = tx.send(WorkerMessage::Done);
    })
}
