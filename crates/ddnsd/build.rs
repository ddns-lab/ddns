use std::process::Command;

fn main() {
    // Get version from git describe
    // Format: v0.1.0-0-g1234567 (or v0.1.0 if on exact tag)
    let version = match Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8(output.stdout).unwrap_or_else(|_| "0.1.0".to_string());

            // Remove 'v' prefix if present
            version.strip_prefix('v').unwrap_or(&version).to_string()
        }
        Err(_) => {
            // Fallback to Cargo.toml version if git fails
            // (e.g., during cargo publish or not in a git repo)
            println!("cargo:rustc-env=GIT_VERSION=0.1.0");
            return;
        }
    };

    // Set GIT_VERSION environment variable for main.rs
    println!("cargo:rustc-env=GIT_VERSION={}", version);

    // Rebuild if git tags change
    println!("cargo:rerun-if-changed=.git/refs/tags");
}
