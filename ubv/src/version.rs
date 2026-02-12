pub fn print_cli_version_banner(tool_name: &str, version: &str, release: &str, commit: &str) {
    println!("{tool_name}");
    println!("Copyright (c) Peter Wright 2020-2026");
    println!("License: GNU AGPL v3 (AGPL-3.0-only)");
    println!("https://github.com/petergeneric/unifi-protect-remux");
    println!();

    println!("\tVersion:     {version}");
    if !release.is_empty() {
        println!("\tGit tag:     {release}");
    }
    if !commit.is_empty() {
        println!("\tGit commit:  {commit}");
    }
}
