use std::process::Command;

fn main() {
    // Inject git commit hash
    let commit = Command::new("git")
        .args(["rev-list", "-1", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_COMMIT={commit}");

    // Inject release version from git tag
    let version = Command::new("git")
        .args(["describe", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=RELEASE_VERSION={version}");
}
