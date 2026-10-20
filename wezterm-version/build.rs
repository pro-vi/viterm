fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // If a file named `.tag` is present, we'll take its contents for the
    // version number that we report in wezterm -h.
    let mut ci_tag = String::new();
    if let Ok(tag) = std::fs::read("../.tag") {
        if let Ok(s) = String::from_utf8(tag) {
            ci_tag = s.trim().to_string();
            println!("cargo:rerun-if-changed=../.tag");
        }
    } else {
        // Otherwise we'll derive it from the git information

        if let Ok(repo) = git2::Repository::discover(".") {
            // HEAD changes when another branch is checked out or a detached
            // HEAD moves, and its reflog on every commit, including one to a
            // branch whose ref is packed. These two live in the worktree's
            // own git directory and the branch ref in the common one; in a
            // plain checkout the two directories are the same `.git`.
            let mut watched = vec![repo.path().join("HEAD"), repo.path().join("logs/HEAD")];
            if let Ok(head) = repo.head() {
                if head.is_branch() {
                    if let Some(name) = head.name() {
                        watched.push(repo.commondir().join(name));
                    }
                }
            }

            // A path that does not exist makes cargo (as of 1.94) rerun
            // this script on every build.
            for path in watched {
                if let Ok(path) = path.canonicalize() {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }

            if let Ok(output) = std::process::Command::new("git")
                .args(&[
                    "-c",
                    "core.abbrev=8",
                    "show",
                    "-s",
                    "--format=%cd-%h",
                    "--date=format:%Y%m%d-%H%M%S",
                ])
                .output()
            {
                let info = String::from_utf8_lossy(&output.stdout);
                ci_tag = info.trim().to_string();
            }
        }
    }

    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=WEZTERM_TARGET_TRIPLE={}", target);
    println!("cargo:rustc-env=WEZTERM_CI_TAG={}", ci_tag);
}
