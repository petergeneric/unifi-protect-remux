use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::{c_char, c_int, c_void};
use std::panic;
use std::sync::Once;

use flate2::write::GzEncoder;
use flate2::Compression;
use remux_lib::{LogLevel, ProgressEvent, RemuxConfig};

extern crate ffmpeg_next;

// ---------------------------------------------------------------------------
// Version info injected at build time
// ---------------------------------------------------------------------------
const GIT_VERSION: &str = env!("GIT_VERSION");
const GIT_COMMIT: &str = env!("GIT_COMMIT");
const LICENSES_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/licenses.json"));

// ---------------------------------------------------------------------------
// JSON serde types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct VersionInfo {
    version: &'static str,
    git_commit: &'static str,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProgressEventJson {
    Log {
        level: String,
        message: String,
    },
    FileStarted {
        path: String,
    },
    PartitionsFound {
        count: usize,
    },
    PartitionStarted {
        index: usize,
        total: usize,
    },
    OutputGenerated {
        path: String,
    },
    PartitionError {
        index: usize,
        error: String,
    },
    FileCompleted {
        path: String,
        outputs: Vec<String>,
        errors: Vec<String>,
    },
}

#[derive(serde::Deserialize)]
struct FfiRemuxConfig {
    with_audio: Option<bool>,
    with_video: Option<bool>,
    force_rate: Option<u32>,
    fast_start: Option<bool>,
    output_folder: Option<String>,
    mp4: Option<bool>,
    video_track: Option<u16>,
    base_name: Option<String>,
}

#[derive(serde::Serialize)]
struct ValidateResult {
    valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(serde::Serialize)]
struct ProcessResult {
    input_path: String,
    output_files: Vec<String>,
    errors: Vec<String>,
}

#[derive(serde::Serialize)]
struct DiagnosticsResult {
    output_path: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CameraData {
    cameras: Vec<CameraDataEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CameraDataEntry {
    mac: String,
    name: String,
}

// ---------------------------------------------------------------------------
// UBV info structured tree types
// ---------------------------------------------------------------------------

/// A single entry row in the info tree (frame, clock sync, or metadata record).
#[derive(serde::Serialize)]
struct UbvInfoEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    track_id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keyframe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dts: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clock_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    packet_position: Option<String>,
}

/// A group of entries (e.g. "Video (H.264)" or "Clock Syncs").
#[derive(serde::Serialize)]
struct UbvInfoGroup {
    label: String,
    count: usize,
    entries: Vec<UbvInfoEntry>,
}

/// Partition header metadata.
#[derive(serde::Serialize)]
struct UbvInfoHeader {
    index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dts: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clock_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format_code: Option<u16>,
    total_entries: usize,
    entry_counts: Vec<UbvInfoGroupCount>,
}

#[derive(serde::Serialize)]
struct UbvInfoGroupCount {
    label: String,
    count: usize,
}

/// A partition node in the info tree.
#[derive(serde::Serialize)]
struct UbvInfoPartition {
    label: String,
    header: UbvInfoHeader,
    groups: Vec<UbvInfoGroup>,
}

/// Top-level structured UBV info response.
#[derive(serde::Serialize)]
struct UbvInfoTree {
    partitions: Vec<UbvInfoPartition>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write an error message into `*error_out` if the pointer is non-null.
///
/// # Safety
///
/// `error_out` must be either null or point to a valid `*mut c_char` location.
unsafe fn set_error(error_out: *mut *mut c_char, msg: &str) {
    if !error_out.is_null() {
        if let Ok(c) = CString::new(msg) {
            unsafe {
                *error_out = c.into_raw();
            }
        }
    }
}

/// Convert a Rust string to a heap-allocated `*mut c_char`.
/// Returns `std::ptr::null_mut()` if the string contains interior NULs.
fn string_to_c(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Convert a `ProgressEvent` from remux-lib into our JSON-friendly enum.
fn event_to_json(event: &ProgressEvent) -> ProgressEventJson {
    match event {
        ProgressEvent::Log(level, msg) => ProgressEventJson::Log {
            level: match level {
                LogLevel::Info => "info".to_string(),
                LogLevel::Warn => "warn".to_string(),
                LogLevel::Error => "error".to_string(),
            },
            message: msg.clone(),
        },
        ProgressEvent::FileStarted { path } => ProgressEventJson::FileStarted {
            path: path.clone(),
        },
        ProgressEvent::PartitionsFound { count } => ProgressEventJson::PartitionsFound {
            count: *count,
        },
        ProgressEvent::PartitionStarted { index, total } => ProgressEventJson::PartitionStarted {
            index: *index,
            total: *total,
        },
        ProgressEvent::OutputGenerated { path } => ProgressEventJson::OutputGenerated {
            path: path.clone(),
        },
        ProgressEvent::PartitionError { index, error } => ProgressEventJson::PartitionError {
            index: *index,
            error: error.clone(),
        },
        ProgressEvent::FileCompleted {
            path,
            outputs,
            errors,
        } => ProgressEventJson::FileCompleted {
            path: path.clone(),
            outputs: outputs.clone(),
            errors: errors.clone(),
        },
    }
}

/// Convert the FFI config JSON into a `RemuxConfig`, filling in defaults for
/// any fields that were not specified.
fn ffi_config_to_remux_config(ffi: &FfiRemuxConfig) -> RemuxConfig {
    let defaults = RemuxConfig::default();
    RemuxConfig {
        with_audio: ffi.with_audio.unwrap_or(defaults.with_audio),
        with_video: ffi.with_video.unwrap_or(defaults.with_video),
        force_rate: ffi.force_rate.unwrap_or(defaults.force_rate),
        fast_start: ffi.fast_start.unwrap_or(defaults.fast_start),
        output_folder: ffi
            .output_folder
            .clone()
            .unwrap_or(defaults.output_folder),
        mp4: ffi.mp4.unwrap_or(defaults.mp4),
        video_track: ffi.video_track.unwrap_or(defaults.video_track),
        base_name: ffi.base_name.clone(),
    }
}

/// Extract the 12-character uppercase hex MAC address from a UBV filename.
///
/// The MAC is the prefix before the first underscore, if it is exactly 12 hex
/// characters.
fn extract_mac(filename: &str) -> Option<String> {
    let underscore_idx = filename.find('_')?;
    if underscore_idx != 12 {
        return None;
    }
    let prefix = &filename[..12];
    if prefix
        .chars()
        .all(|c| c.is_ascii_hexdigit())
    {
        Some(prefix.to_ascii_uppercase())
    } else {
        None
    }
}

/// Extract the unix-milliseconds timestamp string from a UBV filename.
///
/// The timestamp is the last `_`-delimited segment before the `.ubv` (or
/// `.ubv.gz`) extension, and must look like a plausible unix-ms value
/// (between 1e12 and 1e13).
fn extract_timestamp(filename: &str) -> Option<String> {
    let mut name = filename;
    if name.to_ascii_lowercase().ends_with(".gz") {
        name = &name[..name.len() - 3];
    }
    if name.to_ascii_lowercase().ends_with(".ubv") {
        name = &name[..name.len() - 4];
    }

    let last_underscore = name.rfind('_')?;
    let segment = &name[last_underscore + 1..];

    if let Ok(millis) = segment.parse::<i64>() {
        if millis > 1_000_000_000_000 && millis < 10_000_000_000_000 {
            return Some(segment.to_string());
        }
    }
    None
}

/// Format a 12-character hex MAC address with colon separators.
///
/// `"AABBCCDDEEFF"` → `"AA:BB:CC:DD:EE:FF"`.  Returns `None` if the input
/// is not exactly 12 hex characters.
fn format_mac(mac: &str) -> Option<String> {
    if mac.len() != 12 || !mac.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let formatted: Vec<&str> = (0..6).map(|i| &mac[i * 2..i * 2 + 2]).collect();
    Some(formatted.join(":"))
}

/// Sanitise a camera name for use as a filename base.
///
/// Strips characters that are invalid in filenames on Windows/macOS/Linux
/// and trims leading/trailing whitespace.  Returns `None` if the result is
/// empty.
fn sanitize_base_name(name: &str) -> Option<String> {
    const INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let sanitized: String = name.chars().filter(|c| !INVALID.contains(c)).collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Return the platform-specific path for the cameras JSON file.
fn cameras_file_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|d| d.join("RemuxGui").join("cameras.json"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::config_dir().map(|d| d.join("RemuxGui").join("cameras.json"))
    }
}

/// Parse a `.ubv` file and return the structured info tree as JSON.
fn ubv_info(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;
    use ubv::partition::PartitionEntry;

    let ubv_path = std::path::Path::new(path);
    let mut reader = ubv::reader::open_ubv(ubv_path)?;
    let ubv_file = ubv::reader::parse_ubv(&mut reader)?;

    let mut partitions = Vec::new();

    for partition in &ubv_file.partitions {
        let mut frame_groups: BTreeMap<u16, Vec<UbvInfoEntry>> = BTreeMap::new();
        let mut clock_syncs = Vec::new();
        let mut motion = Vec::new();
        let mut smart_events = Vec::new();
        let mut jpegs = Vec::new();
        let mut skips = Vec::new();
        let mut talkback = Vec::new();

        for entry in &partition.entries {
            match entry {
                PartitionEntry::Frame(f) => {
                    let h = &f.header;
                    let row = UbvInfoEntry {
                        entry_type: f.type_char.to_string(),
                        track_id: Some(h.track_id),
                        keyframe: Some(h.keyframe),
                        offset: Some(h.data_offset),
                        size: Some(h.data_size),
                        dts: Some(h.dts),
                        cts: Some(f.cts),
                        wc: Some(f.wc),
                        clock_rate: Some(h.clock_rate),
                        sequence: Some(h.sequence),
                        packet_position: Some(format!("{:?}", f.packet_position)),
                    };
                    frame_groups.entry(h.track_id).or_default().push(row);
                }
                PartitionEntry::ClockSync(cs) => {
                    clock_syncs.push(UbvInfoEntry {
                        entry_type: "CS".to_string(),
                        track_id: None,
                        keyframe: None,
                        offset: None,
                        size: None,
                        dts: Some(cs.sc_dts),
                        cts: None,
                        wc: Some(cs.wc_ms),
                        clock_rate: Some(cs.sc_rate),
                        sequence: None,
                        packet_position: None,
                    });
                }
                PartitionEntry::Motion(m) => motion.push(metadata_to_entry("M", m)),
                PartitionEntry::SmartEvent(m) => smart_events.push(metadata_to_entry("SE", m)),
                PartitionEntry::Jpeg(m) => jpegs.push(metadata_to_entry("J", m)),
                PartitionEntry::Skip(m) => skips.push(metadata_to_entry("Skip", m)),
                PartitionEntry::Talkback(m) => talkback.push(metadata_to_entry("TB", m)),
                _ => {}
            }
        }

        // Build groups in canonical order: frame tracks (sorted), then metadata types
        let mut groups = Vec::new();
        for (track_id, entries) in frame_groups {
            let name = ubv::track::track_display_name(track_id);
            let count = entries.len();
            groups.push(UbvInfoGroup {
                label: format!("{} ({})", name, count),
                count,
                entries,
            });
        }

        fn push_group(groups: &mut Vec<UbvInfoGroup>, label: &str, entries: Vec<UbvInfoEntry>) {
            if !entries.is_empty() {
                let count = entries.len();
                groups.push(UbvInfoGroup {
                    label: format!("{} ({})", label, count),
                    count,
                    entries,
                });
            }
        }

        push_group(&mut groups, "Clock Syncs", clock_syncs);
        push_group(&mut groups, "Motion", motion);
        push_group(&mut groups, "Smart Events", smart_events);
        push_group(&mut groups, "JPEG", jpegs);
        push_group(&mut groups, "Skip", skips);
        push_group(&mut groups, "Talkback", talkback);

        // Build header info
        let total_entries: usize = groups.iter().map(|g| g.count).sum();
        let entry_counts: Vec<UbvInfoGroupCount> = groups
            .iter()
            .map(|g| UbvInfoGroupCount {
                label: g.label.clone(),
                count: g.count,
            })
            .collect();

        let mut header = UbvInfoHeader {
            index: partition.index,
            file_offset: None,
            dts: None,
            clock_rate: None,
            format_code: None,
            total_entries,
            entry_counts,
        };

        if let Some(ph) = &partition.header {
            header.file_offset = Some(ph.file_offset);
            header.dts = Some(ph.dts);
            header.clock_rate = Some(ph.clock_rate);
            header.format_code = Some(ph.format_code.0);
        }

        partitions.push(UbvInfoPartition {
            label: format!("Partition {}", partition.index),
            header,
            groups,
        });
    }

    let tree = UbvInfoTree { partitions };
    let json = serde_json::to_string(&tree)?;
    Ok(json)
}

/// Convert a metadata record into an entry row.
fn metadata_to_entry(display_type: &str, m: &ubv::partition::MetadataRecord) -> UbvInfoEntry {
    let h = &m.header;
    UbvInfoEntry {
        entry_type: display_type.to_string(),
        track_id: Some(h.track_id),
        keyframe: Some(h.keyframe),
        offset: Some(m.file_offset),
        size: Some(h.data_size),
        dts: Some(h.dts),
        cts: None,
        wc: None,
        clock_rate: Some(h.clock_rate),
        sequence: Some(h.sequence),
        packet_position: None,
    }
}

/// Parse and decompress a `.ubv` file, serialise to JSON, gzip-compress,
/// and write to `<path>.json.gz`.
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

// ---------------------------------------------------------------------------
// FFI entry points
// ---------------------------------------------------------------------------

static INIT_ONCE: Once = Once::new();

/// Initialise FFmpeg. Safe to call multiple times; only the first call has
/// any effect.
#[unsafe(no_mangle)]
pub extern "C" fn remux_init() {
    let _ = panic::catch_unwind(|| {
        INIT_ONCE.call_once(|| {
            ffmpeg_next::init().expect("Failed to initialise FFmpeg");
        });
    });
}

/// Return a JSON string containing version information.
///
/// The caller **must** free the returned string with `remux_free_string`.
/// Returns `NULL` on internal error.
#[unsafe(no_mangle)]
pub extern "C" fn remux_version() -> *mut c_char {
    match panic::catch_unwind(|| {
        let info = VersionInfo {
            version: GIT_VERSION,
            git_commit: GIT_COMMIT,
        };
        let json = serde_json::to_string(&info).unwrap_or_default();
        string_to_c(&json)
    }) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Return a JSON array of third-party license information.
///
/// The caller **must** free the returned string with `remux_free_string`.
/// Returns `NULL` on internal error.
#[unsafe(no_mangle)]
pub extern "C" fn remux_licenses() -> *mut c_char {
    match panic::catch_unwind(|| string_to_c(LICENSES_JSON)) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Validate a remux configuration supplied as a JSON string.
///
/// Returns a JSON string: `{"valid":true}` on success, or
/// `{"valid":false,"error":"..."}` on failure.
///
/// The caller **must** free the returned string with `remux_free_string`.
/// Returns `NULL` on internal error.
///
/// # Safety
///
/// `config_json` must be either null or a valid pointer to a NUL-terminated
/// UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_validate_config(config_json: *const c_char) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if config_json.is_null() {
            let result = ValidateResult {
                valid: false,
                error: Some("config_json is NULL".to_string()),
            };
            return string_to_c(&serde_json::to_string(&result).unwrap_or_default());
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                let result = ValidateResult {
                    valid: false,
                    error: Some(format!("Invalid UTF-8 in config_json: {}", e)),
                };
                return string_to_c(&serde_json::to_string(&result).unwrap_or_default());
            }
        };

        let ffi_config: FfiRemuxConfig = match serde_json::from_str(json_str) {
            Ok(c) => c,
            Err(e) => {
                let result = ValidateResult {
                    valid: false,
                    error: Some(format!("Invalid JSON: {}", e)),
                };
                return string_to_c(&serde_json::to_string(&result).unwrap_or_default());
            }
        };

        let config = ffi_config_to_remux_config(&ffi_config);

        match remux_lib::validate_config(&config) {
            Ok(()) => {
                let result = ValidateResult {
                    valid: true,
                    error: None,
                };
                string_to_c(&serde_json::to_string(&result).unwrap_or_default())
            }
            Err(msg) => {
                let result = ValidateResult {
                    valid: false,
                    error: Some(msg),
                };
                string_to_c(&serde_json::to_string(&result).unwrap_or_default())
            }
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Process a single `.ubv` file.
///
/// # Parameters
///
/// - `ubv_path`          - Path to the `.ubv` file (UTF-8 C string).
/// - `config_json`       - Remux configuration as a JSON C string.
/// - `progress_callback` - Called with `(json_event, file_index)` for every
///                         progress event. May be `NULL` to suppress events.
/// - `file_index`        - Opaque index passed through to the callback so
///                         callers can correlate events to files.
/// - `error_out`         - On error, receives a heap-allocated error message.
///                         The caller must free it with `remux_free_string`.
///                         May be `NULL` if the caller does not need it.
///
/// # Returns
///
/// A JSON string describing the result (input path, output files, errors).
/// The caller **must** free the returned string with `remux_free_string`.
/// Returns `NULL` on unrecoverable error (check `*error_out`).
///
/// # Safety
///
/// - `ubv_path` must be either null or a valid NUL-terminated UTF-8 C string.
/// - `config_json` must be either null or a valid NUL-terminated UTF-8 C string.
/// - `error_out` must be either null or point to a valid `*mut c_char` location.
/// - `progress_callback`, if provided, must be safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_process_file(
    ubv_path: *const c_char,
    config_json: *const c_char,
    progress_callback: Option<extern "C" fn(*const c_char, i32)>,
    file_index: i32,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Clear error_out
        if !error_out.is_null() {
            unsafe {
                *error_out = std::ptr::null_mut();
            }
        }

        // Validate ubv_path
        if ubv_path.is_null() {
            unsafe { set_error(error_out, "ubv_path is NULL") };
            return std::ptr::null_mut();
        }
        let ubv_path_str = match unsafe { CStr::from_ptr(ubv_path) }.to_str() {
            Ok(s) => s.to_string(),
            Err(e) => {
                unsafe {
                    set_error(error_out, &format!("Invalid UTF-8 in ubv_path: {}", e));
                }
                return std::ptr::null_mut();
            }
        };

        // Validate config_json
        if config_json.is_null() {
            unsafe { set_error(error_out, "config_json is NULL") };
            return std::ptr::null_mut();
        }
        let config_str = match unsafe { CStr::from_ptr(config_json) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    set_error(
                        error_out,
                        &format!("Invalid UTF-8 in config_json: {}", e),
                    );
                }
                return std::ptr::null_mut();
            }
        };

        // Parse config
        let ffi_config: FfiRemuxConfig = match serde_json::from_str(config_str) {
            Ok(c) => c,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid config JSON: {}", e)) };
                return std::ptr::null_mut();
            }
        };
        let config = ffi_config_to_remux_config(&ffi_config);

        // Validate config
        if let Err(msg) = remux_lib::validate_config(&config) {
            unsafe { set_error(error_out, &msg) };
            return std::ptr::null_mut();
        }

        // Ensure FFmpeg is initialised
        INIT_ONCE.call_once(|| {
            ffmpeg_next::init().expect("Failed to initialise FFmpeg");
        });

        // Process the file, forwarding progress events through the callback
        let mut progress_fn = |event: ProgressEvent| {
            if let Some(cb) = progress_callback {
                let json_event = event_to_json(&event);
                if let Ok(json) = serde_json::to_string(&json_event) {
                    if let Ok(c_json) = CString::new(json) {
                        cb(c_json.as_ptr(), file_index);
                    }
                }
            }
        };

        match remux_lib::process_file(&ubv_path_str, &config, &mut progress_fn) {
            Ok(file_result) => {
                let result = ProcessResult {
                    input_path: file_result.input_path,
                    output_files: file_result.output_files,
                    errors: file_result.errors,
                };
                let json = serde_json::to_string(&result).unwrap_or_default();
                string_to_c(&json)
            }
            Err(e) => {
                unsafe { set_error(error_out, &e.to_string()) };
                std::ptr::null_mut()
            }
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            unsafe { set_error(error_out, "Internal panic during remux_process_file") };
            std::ptr::null_mut()
        }
    }
}

/// Parse a `.ubv` file and produce a gzip-compressed JSON diagnostics file
/// at `<ubv_path>.json.gz`.
///
/// # Parameters
///
/// - `ubv_path`  - Path to the `.ubv` file (UTF-8 C string).
/// - `error_out` - On error, receives a heap-allocated error message.
///                 The caller must free it with `remux_free_string`.
///                 May be `NULL`.
///
/// # Returns
///
/// A JSON string `{"output_path":"..."}` on success. The caller **must**
/// free the returned string with `remux_free_string`.
/// Returns `NULL` on error (check `*error_out`).
///
/// # Safety
///
/// - `ubv_path` must be either null or a valid NUL-terminated UTF-8 C string.
/// - `error_out` must be either null or point to a valid `*mut c_char` location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_produce_diagnostics(
    ubv_path: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Clear error_out
        if !error_out.is_null() {
            unsafe {
                *error_out = std::ptr::null_mut();
            }
        }

        // Validate ubv_path
        if ubv_path.is_null() {
            unsafe { set_error(error_out, "ubv_path is NULL") };
            return std::ptr::null_mut();
        }
        let path_str = match unsafe { CStr::from_ptr(ubv_path) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    set_error(error_out, &format!("Invalid UTF-8 in ubv_path: {}", e));
                }
                return std::ptr::null_mut();
            }
        };

        match produce_diagnostics(path_str) {
            Ok(output_path) => {
                let result = DiagnosticsResult { output_path };
                let json = serde_json::to_string(&result).unwrap_or_default();
                string_to_c(&json)
            }
            Err(e) => {
                unsafe { set_error(error_out, &e.to_string()) };
                std::ptr::null_mut()
            }
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            unsafe {
                set_error(error_out, "Internal panic during remux_produce_diagnostics");
            }
            std::ptr::null_mut()
        }
    }
}

/// Parse a `.ubv` file and return its structure as a JSON string.
///
/// # Parameters
///
/// - `ubv_path`  - Path to the `.ubv` file (UTF-8 C string).
/// - `error_out` - On error, receives a heap-allocated error message.
///                 The caller must free it with `remux_free_string`.
///                 May be `NULL`.
///
/// # Returns
///
/// A JSON string containing the parsed UBV file structure. The caller
/// **must** free the returned string with `remux_free_string`.
/// Returns `NULL` on error (check `*error_out`).
///
/// # Safety
///
/// - `ubv_path` must be either null or a valid NUL-terminated UTF-8 C string.
/// - `error_out` must be either null or point to a valid `*mut c_char` location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_ubv_info(
    ubv_path: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Clear error_out
        if !error_out.is_null() {
            unsafe {
                *error_out = std::ptr::null_mut();
            }
        }

        // Validate ubv_path
        if ubv_path.is_null() {
            unsafe { set_error(error_out, "ubv_path is NULL") };
            return std::ptr::null_mut();
        }
        let path_str = match unsafe { CStr::from_ptr(ubv_path) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    set_error(error_out, &format!("Invalid UTF-8 in ubv_path: {}", e));
                }
                return std::ptr::null_mut();
            }
        };

        match ubv_info(path_str) {
            Ok(json) => string_to_c(&json),
            Err(e) => {
                unsafe { set_error(error_out, &e.to_string()) };
                std::ptr::null_mut()
            }
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            unsafe {
                set_error(error_out, "Internal panic during remux_ubv_info");
            }
            std::ptr::null_mut()
        }
    }
}

/// Extract a JPEG thumbnail from the first video frame of an MP4 file.
///
/// # Parameters
///
/// - `mp4_path`    - Path to the MP4 file (UTF-8 C string).
/// - `output_path` - Where to write the JPEG thumbnail (UTF-8 C string).
/// - `max_width`   - Maximum thumbnail width in pixels.
/// - `error_out`   - On error, receives a heap-allocated error message.
///                   The caller must free it with `remux_free_string`.
///                   May be `NULL`.
///
/// # Returns
///
/// `0` on success, non-zero on error (check `*error_out`).
///
/// # Safety
///
/// - `mp4_path` and `output_path` must be valid NUL-terminated UTF-8 C strings.
/// - `error_out` must be either null or point to a valid `*mut c_char` location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_extract_thumbnail(
    mp4_path: *const c_char,
    output_path: *const c_char,
    max_width: u32,
    error_out: *mut *mut c_char,
) -> i32 {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !error_out.is_null() {
            unsafe {
                *error_out = std::ptr::null_mut();
            }
        }

        if mp4_path.is_null() {
            unsafe { set_error(error_out, "mp4_path is NULL") };
            return 1;
        }
        let mp4_str = match unsafe { CStr::from_ptr(mp4_path) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid UTF-8 in mp4_path: {}", e)) };
                return 1;
            }
        };

        if output_path.is_null() {
            unsafe { set_error(error_out, "output_path is NULL") };
            return 1;
        }
        let out_str = match unsafe { CStr::from_ptr(output_path) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    set_error(error_out, &format!("Invalid UTF-8 in output_path: {}", e));
                }
                return 1;
            }
        };

        // Ensure FFmpeg is initialised
        INIT_ONCE.call_once(|| {
            ffmpeg_next::init().expect("Failed to initialise FFmpeg");
        });

        match remux_lib::thumbnail::extract_thumbnail(mp4_str, out_str, max_width) {
            Ok(()) => 0,
            Err(e) => {
                unsafe { set_error(error_out, &e.to_string()) };
                1
            }
        }
    })) {
        Ok(code) => code,
        Err(_) => {
            unsafe { set_error(error_out, "Internal panic during remux_extract_thumbnail") };
            1
        }
    }
}

/// Extract the 12-character uppercase hex MAC address from a UBV filename.
///
/// Returns a heap-allocated string on success, `NULL` if no MAC was found.
/// The caller **must** free the returned string with `remux_free_string`.
///
/// # Safety
///
/// `filename` must be either null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_extract_mac(filename: *const c_char) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if filename.is_null() {
            return std::ptr::null_mut();
        }
        let c_str = unsafe { CStr::from_ptr(filename) };
        let name = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        match extract_mac(name) {
            Some(mac) => string_to_c(&mac),
            None => std::ptr::null_mut(),
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Extract the unix-milliseconds timestamp from a UBV filename as a string.
///
/// Returns a heap-allocated string on success, `NULL` if no timestamp was
/// found. The caller **must** free the returned string with
/// `remux_free_string`.
///
/// # Safety
///
/// `filename` must be either null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_extract_timestamp(filename: *const c_char) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if filename.is_null() {
            return std::ptr::null_mut();
        }
        let c_str = unsafe { CStr::from_ptr(filename) };
        let name = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        match extract_timestamp(name) {
            Some(ts) => string_to_c(&ts),
            None => std::ptr::null_mut(),
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Check whether a UBV filename indicates a low-resolution recording.
///
/// Returns `1` if the filename contains `_2_rotating_` or `_timelapse_`
/// (case-insensitive), `0` otherwise.
///
/// # Safety
///
/// `filename` must be either null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_is_low_res_filename(filename: *const c_char) -> c_int {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if filename.is_null() {
            return 0;
        }
        let c_str = unsafe { CStr::from_ptr(filename) };
        let name = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let lower = name.to_ascii_lowercase();
        if lower.contains("_2_rotating_") || lower.contains("_timelapse_") {
            1
        } else {
            0
        }
    })) {
        Ok(v) => v,
        Err(_) => 0,
    }
}

/// Format a 12-character hex MAC address with colon separators.
///
/// `"AABBCCDDEEFF"` → `"AA:BB:CC:DD:EE:FF"`.
///
/// Returns a heap-allocated string on success, `NULL` if the input is not
/// exactly 12 hex characters. The caller **must** free the returned string
/// with `remux_free_string`.
///
/// # Safety
///
/// `mac` must be either null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_format_mac(mac: *const c_char) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if mac.is_null() {
            return std::ptr::null_mut();
        }
        let c_str = unsafe { CStr::from_ptr(mac) };
        let s = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        match format_mac(s) {
            Some(formatted) => string_to_c(&formatted),
            None => std::ptr::null_mut(),
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Sanitise a string for use as a filename base.
///
/// Strips characters that are invalid in filenames on common platforms
/// (`/\:*?"<>|`) and trims whitespace. Returns `NULL` if the result would
/// be empty.
///
/// The caller **must** free the returned string with `remux_free_string`.
///
/// # Safety
///
/// `name` must be either null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_sanitize_base_name(name: *const c_char) -> *mut c_char {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if name.is_null() {
            return std::ptr::null_mut();
        }
        let c_str = unsafe { CStr::from_ptr(name) };
        let s = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        match sanitize_base_name(s) {
            Some(sanitized) => string_to_c(&sanitized),
            None => std::ptr::null_mut(),
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Load the cameras registry from the platform-specific application data
/// directory.
///
/// Returns a JSON string `{"cameras":[...]}`. If the file does not exist or
/// cannot be read, returns `{"cameras":[]}`.
///
/// The caller **must** free the returned string with `remux_free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn remux_load_cameras() -> *mut c_char {
    match panic::catch_unwind(|| {
        let data = (|| -> CameraData {
            let path = match cameras_file_path() {
                Some(p) => p,
                None => return CameraData { cameras: vec![] },
            };
            let json = match std::fs::read_to_string(&path) {
                Ok(j) => j,
                Err(_) => return CameraData { cameras: vec![] },
            };
            match serde_json::from_str(&json) {
                Ok(d) => d,
                Err(_) => CameraData { cameras: vec![] },
            }
        })();
        let json = serde_json::to_string(&data).unwrap_or_else(|_| r#"{"cameras":[]}"#.to_string());
        string_to_c(&json)
    }) {
        Ok(ptr) => ptr,
        Err(_) => string_to_c(r#"{"cameras":[]}"#),
    }
}

/// Save the cameras registry to the platform-specific application data
/// directory.
///
/// # Parameters
///
/// - `cameras_json` - JSON string `{"cameras":[{"mac":"...","name":"..."},
///   ...]}`.
/// - `error_out`    - On error, receives a heap-allocated error message. May
///   be `NULL`.
///
/// # Returns
///
/// `0` on success, non-zero on error.
///
/// # Safety
///
/// - `cameras_json` must be either null or a valid NUL-terminated UTF-8 C
///   string.
/// - `error_out` must be either null or point to a valid `*mut c_char`
///   location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_save_cameras(
    cameras_json: *const c_char,
    error_out: *mut *mut c_char,
) -> c_int {
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !error_out.is_null() {
            unsafe { *error_out = std::ptr::null_mut(); }
        }

        if cameras_json.is_null() {
            unsafe { set_error(error_out, "cameras_json is NULL"); }
            return 1;
        }
        let c_str = unsafe { CStr::from_ptr(cameras_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid UTF-8: {}", e)); }
                return 1;
            }
        };

        // Validate the JSON parses correctly
        let _data: CameraData = match serde_json::from_str(json_str) {
            Ok(d) => d,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid JSON: {}", e)); }
                return 1;
            }
        };

        let path = match cameras_file_path() {
            Some(p) => p,
            None => {
                unsafe { set_error(error_out, "Cannot determine application data directory"); }
                return 1;
            }
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                unsafe { set_error(error_out, &format!("Failed to create directory: {}", e)); }
                return 1;
            }
        }

        match std::fs::write(&path, json_str) {
            Ok(()) => 0,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Failed to write file: {}", e)); }
                1
            }
        }
    })) {
        Ok(code) => code,
        Err(_) => {
            unsafe { set_error(error_out, "Internal panic during remux_save_cameras"); }
            1
        }
    }
}

/// Process a single `.ubv` file (with context pointer for callbacks).
///
/// This is identical to `remux_process_file` except the callback receives an
/// additional `*mut c_void` context pointer, allowing callers (e.g. Swift) to
/// pass closure context without using globals.
///
/// # Safety
///
/// Same requirements as `remux_process_file`, plus `context` is passed
/// through opaquely to the callback and must remain valid for the duration of
/// the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_process_file_ctx(
    ubv_path: *const c_char,
    config_json: *const c_char,
    progress_callback: Option<extern "C" fn(*const c_char, c_int, *mut c_void)>,
    file_index: c_int,
    context: *mut c_void,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    // Wrap context in a Send-able newtype so catch_unwind is satisfied
    struct SendCtx(*mut c_void);
    unsafe impl Send for SendCtx {}

    let ctx = SendCtx(context);

    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Clear error_out
        if !error_out.is_null() {
            unsafe { *error_out = std::ptr::null_mut(); }
        }

        // Validate ubv_path
        if ubv_path.is_null() {
            unsafe { set_error(error_out, "ubv_path is NULL") };
            return std::ptr::null_mut();
        }
        let ubv_path_str = match unsafe { CStr::from_ptr(ubv_path) }.to_str() {
            Ok(s) => s.to_string(),
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid UTF-8 in ubv_path: {}", e)); }
                return std::ptr::null_mut();
            }
        };

        // Validate config_json
        if config_json.is_null() {
            unsafe { set_error(error_out, "config_json is NULL") };
            return std::ptr::null_mut();
        }
        let config_str = match unsafe { CStr::from_ptr(config_json) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid UTF-8 in config_json: {}", e)); }
                return std::ptr::null_mut();
            }
        };

        // Parse config
        let ffi_config: FfiRemuxConfig = match serde_json::from_str(config_str) {
            Ok(c) => c,
            Err(e) => {
                unsafe { set_error(error_out, &format!("Invalid config JSON: {}", e)) };
                return std::ptr::null_mut();
            }
        };
        let config = ffi_config_to_remux_config(&ffi_config);

        // Validate config
        if let Err(msg) = remux_lib::validate_config(&config) {
            unsafe { set_error(error_out, &msg) };
            return std::ptr::null_mut();
        }

        // Ensure FFmpeg is initialised
        INIT_ONCE.call_once(|| {
            ffmpeg_next::init().expect("Failed to initialise FFmpeg");
        });

        // Process the file, forwarding progress events through the callback
        let raw_ctx = ctx.0;
        let mut progress_fn = |event: ProgressEvent| {
            if let Some(cb) = progress_callback {
                let json_event = event_to_json(&event);
                if let Ok(json) = serde_json::to_string(&json_event) {
                    if let Ok(c_json) = CString::new(json) {
                        cb(c_json.as_ptr(), file_index, raw_ctx);
                    }
                }
            }
        };

        match remux_lib::process_file(&ubv_path_str, &config, &mut progress_fn) {
            Ok(file_result) => {
                let result = ProcessResult {
                    input_path: file_result.input_path,
                    output_files: file_result.output_files,
                    errors: file_result.errors,
                };
                let json = serde_json::to_string(&result).unwrap_or_default();
                string_to_c(&json)
            }
            Err(e) => {
                unsafe { set_error(error_out, &e.to_string()) };
                std::ptr::null_mut()
            }
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            unsafe { set_error(error_out, "Internal panic during remux_process_file_ctx") };
            std::ptr::null_mut()
        }
    }
}

/// Free a string that was returned by one of the `remux_*` functions.
///
/// Passing `NULL` is safe and has no effect.
///
/// # Safety
///
/// `s` must be either null or a pointer previously returned by one of the
/// `remux_*` functions in this library. Each pointer must only be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remux_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}
