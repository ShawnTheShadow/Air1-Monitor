use std::process::Command;

fn main() {
    // Get git commit count
    let commit_count = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // Set the version with git suffix
    println!("cargo:rustc-env=CARGO_PKG_VERSION_GIT={}", commit_count);
    
    // Rerun if git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
