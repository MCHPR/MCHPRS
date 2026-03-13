use std::process::Command;

fn main() {
    let git_hash = if let Ok(output) = Command::new("git").args(&["rev-parse", "HEAD"]).output() {
        String::from_utf8(output.stdout).unwrap()
    } else {
        "unknown git hash".to_owned()
    };
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
