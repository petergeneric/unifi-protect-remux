#[path = "../build/shared_git_metadata.rs"]
mod shared_git_metadata;

fn main() {
    shared_git_metadata::emit_git_metadata();
}
