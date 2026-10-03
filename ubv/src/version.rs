pub fn print_cli_version_banner(tool_name: &str, version: &str, commit: &str) {
    println!("{tool_name}");
    println!("Copyright (c) Peter Wright 2020-2026");
    println!("License: GNU AGPL v3 (AGPL-3.0-only)");
    println!("https://github.com/petergeneric/unifi-protect-remux");
    println!();

    if !version.is_empty() {
        println!("\tVersion:     {version}");
    }
    if !commit.is_empty() {
        println!("\tGit commit:  {commit}");
    }
}

/// Version of the `ubv-info --json` output shape, bumped when it changes:
/// 1 = before untimed records and `read_status`; 2 = adds `Untimed` entries and
/// `read_status`.
pub const JSON_FORMAT_VERSION: u32 = 2;

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Machine-readable build identity, one JSON object on one line:
/// `{"tool":…,"version":…,"commit":…,"dirty":…[,"format_version":…]}`.
///
/// - `commit`: full hash of HEAD, or `"unknown"` when built without git.
/// - `dirty`: `true` if the build had changes not committed, `null` if unknown.
///   A consumer must not cache results keyed on a build that is dirty or unknown.
pub fn cli_version_json(
    tool: &str,
    version: &str,
    commit: &str,
    dirty: &str,
    format_version: Option<u32>,
) -> String {
    let commit = if commit.is_empty() { "unknown" } else { commit };
    let dirty = match dirty {
        "true" => "true",
        "false" => "false",
        _ => "null",
    };
    let mut out = format!(
        "{{\"tool\":{},\"version\":{},\"commit\":{},\"dirty\":{}",
        json_string(tool),
        json_string(version),
        json_string(commit),
        dirty
    );
    if let Some(v) = format_version {
        out.push_str(&format!(",\"format_version\":{v}"));
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_json_shapes() {
        assert_eq!(
            cli_version_json("ubv-info", "v1-2-gabc-dirty", "abc", "true", Some(2)),
            r#"{"tool":"ubv-info","version":"v1-2-gabc-dirty","commit":"abc","dirty":true,"format_version":2}"#
        );
        assert_eq!(
            cli_version_json("remux", "", "", "", None),
            r#"{"tool":"remux","version":"","commit":"unknown","dirty":null}"#
        );
        assert_eq!(json_string("a\"b\\c\n"), r#""a\"b\\c\u000a""#);
    }
}
