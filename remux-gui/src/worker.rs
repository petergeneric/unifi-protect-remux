use std::sync::mpsc;

use remux_lib::{ProgressEvent, RemuxConfig};

pub enum WorkerMessage {
    Progress { file_index: usize, event: ProgressEvent },
    Done,
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
