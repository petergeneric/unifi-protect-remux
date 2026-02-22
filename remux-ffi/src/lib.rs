use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::c_char;
use std::panic;
use std::sync::Once;

use flate2::write::GzEncoder;
use flate2::Compression;
use remux_lib::{LogLevel, ProgressEvent, RemuxConfig};

extern crate ffmpeg_next;

// ---------------------------------------------------------------------------
// Version info injected at build time
// ---------------------------------------------------------------------------
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_COMMIT: &str = env!("GIT_COMMIT");
const RELEASE_VERSION: &str = env!("RELEASE_VERSION");

// ---------------------------------------------------------------------------
// JSON serde types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct VersionInfo {
    version: &'static str,
    git_commit: &'static str,
    release_version: &'static str,
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
            version: VERSION,
            git_commit: GIT_COMMIT,
            release_version: RELEASE_VERSION,
        };
        let json = serde_json::to_string(&info).unwrap_or_default();
        string_to_c(&json)
    }) {
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
