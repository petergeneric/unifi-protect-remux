use std::process::Command;

pub fn emit_git_metadata() {
    // Re-run when git state changes (commit, tag, branch) so cached
    // CI builds pick up the correct version after tagging.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../build/shared_git_metadata.rs");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");
    println!("cargo:rerun-if-changed=../.git/packed-refs");

    // Inject git commit hash.
    let commit = Command::new("git")
        .args(["rev-list", "-1", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_COMMIT={commit}");

    // Inject release version (only if HEAD is directly tagged).
    let version = Command::new("git")
        .args(["tag", "--points-at", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=RELEASE_VERSION={version}");
}
