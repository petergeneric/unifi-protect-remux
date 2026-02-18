mod worker;

use std::sync::mpsc;

use eframe::egui;
use remux_lib::{LogLevel, ProgressEvent, RemuxConfig};
use worker::{DiagnosticsMessage, WorkerMessage};

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/appicon.png"))
            .expect("Failed to decode app icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_drag_and_drop(true)
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "UBV Remux",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.interaction.selectable_labels = false;
            style.interaction.multi_widget_text_select = false;
            cc.egui_ctx.set_style(style);
            Ok(Box::new(RemuxGuiApp::default()))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
struct QueuedFile {
    path: String,
    status: FileStatus,
    output_files: Vec<String>,
    error: Option<String>,
}

struct RemuxGuiApp {
    files: Vec<QueuedFile>,
    config: RemuxConfig,
    processing: bool,
    progress_rx: Option<mpsc::Receiver<WorkerMessage>>,
    diagnostics_rx: Option<mpsc::Receiver<DiagnosticsMessage>>,
    diagnostics_processing: bool,
    show_settings: bool,
    show_about: bool,
    log_lines: Vec<(LogLevel, String)>,
    output_files: Vec<String>,
    current_file_index: Option<usize>,
}

impl Default for RemuxGuiApp {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            config: RemuxConfig {
                output_folder: "SRC-FOLDER".to_string(),
                ..Default::default()
            },
            processing: false,
            progress_rx: None,
            diagnostics_rx: None,
            diagnostics_processing: false,
            show_settings: false,
            show_about: false,
            log_lines: Vec::new(),
            output_files: Vec::new(),
            current_file_index: None,
        }
    }
}

impl RemuxGuiApp {
    fn add_files(&mut self, paths: Vec<String>) {
        let mut warned_paths = Vec::new();

        for path in paths {
            let lower = path.to_lowercase();
            if lower.ends_with(".ubv") || lower.ends_with(".ubv.gz") {
                if !self.files.iter().any(|f| f.path == path) {
                    if lower.contains("_2_rotating_") || lower.contains("_timelapse_") {
                        warned_paths.push(path);
                    } else {
                        self.files.push(QueuedFile {
                            path,
                            status: FileStatus::Pending,
                            output_files: Vec::new(),
                            error: None,
                        });
                    }
                }
            }
        }

        if !warned_paths.is_empty() {
            let file_names: Vec<String> = warned_paths
                .iter()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.clone())
                })
                .collect();

            let result = rfd::MessageDialog::new()
                .set_title("Low-Resolution File Warning")
                .set_description(format!(
                    "The following file(s) appear to be low-resolution recordings \
                     that do not contain the raw camera data:\n\n\
                     {}\n\n\
                     These files are unlikely to produce useful results, and the \
                     remux tool does not fully support them.\n\n\
                     Add them anyway?",
                    file_names.join("\n")
                ))
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();

            if result == rfd::MessageDialogResult::Yes {
                for path in warned_paths {
                    self.files.push(QueuedFile {
                        path,
                        status: FileStatus::Pending,
                        output_files: Vec::new(),
                        error: None,
                    });
                }
            }
        }
    }

    fn start_processing(&mut self) {
        if self.files.is_empty() || self.processing {
            return;
        }

        self.log_lines.clear();
        self.output_files.clear();

        let file_paths: Vec<String> = self.files.iter().map(|f| f.path.clone()).collect();
        for f in &mut self.files {
            f.status = FileStatus::Pending;
            f.output_files.clear();
            f.error = None;
        }

        let config = RemuxConfig {
            with_audio: self.config.with_audio,
            with_video: self.config.with_video,
            force_rate: self.config.force_rate,
            fast_start: self.config.fast_start,
            output_folder: self.config.output_folder.clone(),
            mp4: self.config.mp4,
            video_track: self.config.video_track,
        };

        let (tx, rx) = mpsc::channel();
        self.progress_rx = Some(rx);
        self.processing = true;
        self.current_file_index = None;

        worker::spawn(file_paths, config, tx);
    }

    fn poll_worker(&mut self) {
        // Collect messages first to avoid borrow conflict
        let messages: Vec<WorkerMessage> = match &self.progress_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        for msg in messages {
            match msg {
                WorkerMessage::Progress { file_index, event } => {
                    self.handle_progress(file_index, event);
                }
                WorkerMessage::Done => {
                    self.processing = false;
                    self.progress_rx = None;
                    self.current_file_index = None;
                    return;
                }
            }
        }
    }

    fn handle_progress(&mut self, file_index: usize, event: ProgressEvent) {
        match event {
            ProgressEvent::Log(level, msg) => {
                self.log_lines.push((level, msg));
            }
            ProgressEvent::FileStarted { .. } => {
                if let Some(f) = self.files.get_mut(file_index) {
                    f.status = FileStatus::Processing;
                }
                self.current_file_index = Some(file_index);
            }
            ProgressEvent::PartitionsFound { count } => {
                self.log_lines.push((
                    LogLevel::Info,
                    format!("Found {} partition(s)", count),
                ));
            }
            ProgressEvent::PartitionStarted { index, total } => {
                self.log_lines.push((
                    LogLevel::Info,
                    format!("Processing partition {}/{}", index + 1, total),
                ));
            }
            ProgressEvent::OutputGenerated { path } => {
                if let Some(f) = self.files.get_mut(file_index) {
                    f.output_files.push(path.clone());
                }
                self.output_files.push(path);
            }
            ProgressEvent::PartitionError { index, error } => {
                self.log_lines
                    .push((LogLevel::Error, format!("Partition #{}: {}", index, error)));
            }
            ProgressEvent::FileCompleted { errors, .. } => {
                if let Some(f) = self.files.get_mut(file_index) {
                    if errors.is_empty() {
                        f.status = FileStatus::Completed;
                    } else {
                        f.status = FileStatus::Failed;
                        f.error = Some(errors.join("; "));
                    }
                }
            }
        }
    }

    fn start_diagnostics(&mut self) {
        if self.files.is_empty() || self.processing || self.diagnostics_processing {
            return;
        }

        self.log_lines.clear();
        self.output_files.clear();

        let file_paths: Vec<String> = self.files.iter().map(|f| f.path.clone()).collect();
        for f in &mut self.files {
            f.status = FileStatus::Pending;
            f.output_files.clear();
            f.error = None;
        }

        let (tx, rx) = mpsc::channel();
        self.diagnostics_rx = Some(rx);
        self.diagnostics_processing = true;

        worker::spawn_diagnostics(file_paths, tx);
    }

    fn poll_diagnostics(&mut self) {
        let messages: Vec<DiagnosticsMessage> = match &self.diagnostics_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        for msg in messages {
            match msg {
                DiagnosticsMessage::FileStarted { file_index } => {
                    if let Some(f) = self.files.get_mut(file_index) {
                        f.status = FileStatus::Processing;
                    }
                    self.log_lines.push((
                        LogLevel::Info,
                        format!("Producing diagnostics for file {}...", file_index + 1),
                    ));
                }
                DiagnosticsMessage::FileCompleted {
                    file_index,
                    output_path,
                } => {
                    if let Some(f) = self.files.get_mut(file_index) {
                        f.status = FileStatus::Completed;
                        f.output_files.push(output_path.clone());
                    }
                    self.output_files.push(output_path);
                }
                DiagnosticsMessage::FileFailed { file_index, error } => {
                    if let Some(f) = self.files.get_mut(file_index) {
                        f.status = FileStatus::Failed;
                        f.error = Some(error.clone());
                    }
                    self.log_lines.push((LogLevel::Error, error));
                }
                DiagnosticsMessage::Done => {
                    self.diagnostics_processing = false;
                    self.diagnostics_rx = None;
                    return;
                }
            }
        }
    }
}

/// Format a file status into a display string.
fn status_label(file: &QueuedFile) -> String {
    match file.status {
        FileStatus::Pending => "Pending".to_string(),
        FileStatus::Processing => "Processing...".to_string(),
        FileStatus::Completed if !file.output_files.is_empty() => {
            let n = file.output_files.len();
            if n == 1 {
                "Done (1 file)".to_string()
            } else {
                format!("Done ({} files)", n)
            }
        }
        FileStatus::Completed => "Done".to_string(),
        FileStatus::Failed => "Failed".to_string(),
    }
}

fn status_color(status: FileStatus, ui: &egui::Ui) -> egui::Color32 {
    match status {
        FileStatus::Failed => egui::Color32::from_rgb(255, 100, 100),
        FileStatus::Processing => egui::Color32::from_rgb(255, 200, 60),
        FileStatus::Completed => egui::Color32::from_rgb(100, 220, 100),
        FileStatus::Pending => ui.visuals().weak_text_color(),
    }
}

/// Draw a collapsible section header with a separator line.
fn section_heading(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    ui.strong(label);
    ui.add_space(2.0);
}

impl eframe::App for RemuxGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.processing {
            self.poll_worker();
            ctx.request_repaint();
        }
        if self.diagnostics_processing {
            self.poll_diagnostics();
            ctx.request_repaint();
        }

        // Handle drag-and-drop
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw.dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().to_string()))
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }

        // -- Top bar --
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.heading("UBV Remux");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.show_settings, "Settings")
                        .clicked()
                    {
                        self.show_settings = !self.show_settings;
                    }
                    ui.menu_button("Help", |ui| {
                        if ui.button("About").clicked() {
                            self.show_about = true;
                            ui.close_menu();
                        }
                    });
                });
            });
            ui.add_space(2.0);
        });

        // -- About window --
        if self.show_about {
            let mut open = self.show_about;
            egui::Window::new("About UBV Remux")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.heading("UBV Remux");
                    });
                    ui.add_space(8.0);

                    egui::Grid::new("about_grid")
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong("Version:");
                            ui.label(env!("CARGO_PKG_VERSION"));
                            ui.end_row();

                            let release = env!("RELEASE_VERSION");
                            if !release.is_empty() {
                                ui.strong("Release:");
                                ui.label(release);
                                ui.end_row();
                            }

                            let commit = env!("GIT_COMMIT");
                            if !commit.is_empty() {
                                ui.strong("Git commit:");
                                let short = if commit.len() > 10 {
                                    &commit[..10]
                                } else {
                                    commit
                                };
                                ui.label(egui::RichText::new(short).monospace());
                                ui.end_row();
                            }

                            ui.strong("License:");
                            ui.label("AGPL-3.0-only");
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label("Copyright (c) Peter Wright 2020-2026");

                    ui.add_space(4.0);

                    ui.hyperlink_to(
                        "github.com/petergeneric/unifi-protect-remux",
                        "https://github.com/petergeneric/unifi-protect-remux",
                    );

                    ui.add_space(4.0);

                    ui.label(
                        egui::RichText::new(
                            "Converts Ubiquiti .ubv video files to standard MP4 \
                             via remuxing.",
                        )
                        .weak(),
                    );
                });
            self.show_about = open;
        }

        // -- Settings side panel --
        if self.show_settings {
            egui::SidePanel::right("settings")
                .resizable(false)
                .min_width(250.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.heading("Settings");
                    ui.separator();

                    ui.add_space(4.0);
                    ui.checkbox(&mut self.config.with_audio, "Extract audio");
                    ui.checkbox(&mut self.config.with_video, "Extract video");
                    ui.checkbox(&mut self.config.fast_start, "Fast start (moov atom at front)");
                    ui.checkbox(&mut self.config.mp4, "MP4 output");

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label("Force framerate (0 = auto VFR):");
                    let mut rate = self.config.force_rate as i32;
                    ui.add(egui::DragValue::new(&mut rate).range(0..=240));
                    self.config.force_rate = rate.max(0) as u32;

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label("Video track (0 = auto-detect):");
                    let mut track = self.config.video_track as i32;
                    ui.add(egui::DragValue::new(&mut track).range(0..=65535));
                    self.config.video_track = track.max(0) as u16;

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label("Output folder:");
                    ui.text_edit_singleline(&mut self.config.output_folder);
                    ui.add_space(2.0);
                    if ui.button("Browse...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.config.output_folder = folder.to_string_lossy().to_string();
                        }
                    }
                });
        }

        // -- Bottom bar: action buttons --
        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let busy = self.processing || self.diagnostics_processing;
                if ui
                    .add_enabled(
                        !busy && !self.files.is_empty(),
                        egui::Button::new("Start"),
                    )
                    .clicked()
                {
                    self.start_processing();
                }
                if ui
                    .add_enabled(
                        !busy && !self.files.is_empty(),
                        egui::Button::new("Produce Diagnostics"),
                    )
                    .clicked()
                {
                    self.start_diagnostics();
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Clear"))
                    .clicked()
                {
                    self.files.clear();
                    self.log_lines.clear();
                    self.output_files.clear();
                }

                if busy {
                    ui.spinner();
                }
            });
            ui.add_space(6.0);
        });

        // -- Central panel --
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("main_scroll")
                .show(ui, |ui| {
                    // -- Drop zone --
                    let drop_frame = egui::Frame::group(ui.style())
                        .inner_margin(16.0);
                    drop_frame.show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0);
                            ui.label("Drop .ubv files here, or click to browse");
                            ui.add_space(6.0);
                            if ui
                                .add_enabled(
                                    !self.processing,
                                    egui::Button::new("Browse Files..."),
                                )
                                .clicked()
                            {
                                if let Some(paths) = rfd::FileDialog::new()
                                    .add_filter("UBV files", &["ubv", "gz"])
                                    .pick_files()
                                {
                                    let paths: Vec<String> = paths
                                        .iter()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .collect();
                                    self.add_files(paths);
                                }
                            }
                            ui.add_space(4.0);
                        });
                    });

                    // -- File list --
                    if !self.files.is_empty() {
                        section_heading(ui, "Files");
                        let file_frame = egui::Frame::group(ui.style())
                            .inner_margin(6.0);
                        file_frame.show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            for (i, file) in self.files.iter().enumerate() {
                                if i > 0 {
                                    ui.separator();
                                }
                                ui.horizontal(|ui| {
                                    let name = std::path::Path::new(&file.path)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| file.path.clone());
                                    ui.label(&name);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let color = status_color(file.status, ui);
                                            ui.colored_label(color, status_label(file));
                                        },
                                    );
                                });
                            }
                        });
                    }

                    // -- Log output --
                    if !self.log_lines.is_empty() {
                        section_heading(ui, "Log");
                        let log_frame = egui::Frame::group(ui.style())
                            .fill(ui.visuals().extreme_bg_color)
                            .inner_margin(8.0);
                        log_frame.show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            egui::ScrollArea::vertical()
                                .id_salt("log_scroll")
                                .max_height(200.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for (level, msg) in &self.log_lines {
                                        let (prefix, color) = match level {
                                            LogLevel::Info => {
                                                ("[INFO]", ui.visuals().text_color())
                                            }
                                            LogLevel::Warn => {
                                                ("[WARN]", egui::Color32::from_rgb(255, 200, 60))
                                            }
                                            LogLevel::Error => {
                                                ("[ERROR]", egui::Color32::from_rgb(255, 100, 100))
                                            }
                                        };
                                        let text = egui::RichText::new(format!(
                                            "{} {}",
                                            prefix, msg
                                        ))
                                        .monospace()
                                        .color(color);
                                        ui.label(text);
                                    }
                                });
                        });
                    }

                    // -- Output files --
                    if !self.output_files.is_empty() {
                        section_heading(ui, "Output Files");
                        let out_frame = egui::Frame::group(ui.style())
                            .inner_margin(8.0);
                        out_frame.show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            for (i, path) in self.output_files.iter().enumerate() {
                                if i > 0 {
                                    ui.separator();
                                }
                                ui.label(
                                    egui::RichText::new(path).monospace(),
                                );
                            }
                        });
                    }

                    ui.add_space(8.0);
                });
        });
    }
}
