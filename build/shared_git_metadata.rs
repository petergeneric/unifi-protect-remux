use std::process::Command;

pub fn emit_git_metadata() {
    // Re-run when git state changes (commit, tag, branch) so cached
    // CI builds pick up the correct version after tagging.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../build/shared_git_metadata.rs");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");
    println!("cargo:rerun-if-changed=../.git/packed-refs");
    // The dirty flag below depends on the working tree, so re-run when any
    // source that goes into the binaries changes (directories are scanned
    // recursively by cargo), or when the index changes (staging, commits).
    println!("cargo:rerun-if-changed=../.git/index");
    for dir in [
        "../ubv",
        "../ubv-info",
        "../remux",
        "../remux-lib",
        "../ubv-anonymise",
        "../create-ubv",
        "../build",
        "../Cargo.toml",
        "../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={dir}");
    }

    // Inject git commit hash.
    let commit = Command::new("git")
        .args(["rev-list", "-1", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_COMMIT={commit}");

    // Inject version derived from git tags via `git describe`.
    // Produces e.g. "v4.2.0" on a tag, "v4.2.0-3-gabcdef" when past a tag,
    // or a short commit hash if no tags exist.
    let version = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_VERSION={version}");

    // "true" when the working tree has changes not in HEAD (tracked or
    // untracked, ignored files excepted), "false" when clean, "" without git.
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| (!o.stdout.iter().all(u8::is_ascii_whitespace)).to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_DIRTY={dirty}");
}
