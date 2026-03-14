use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../sidecars/cliproxy");
    println!("cargo:rerun-if-changed=../scripts/build-cliproxy.sh");
    println!("cargo:rerun-if-env-changed=ATM_SKIP_CLIPROXY_BUILD");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");

    if env::var("ATM_SKIP_CLIPROXY_BUILD").ok().as_deref() != Some("1") {
        build_cliproxy();
    } else {
        println!("cargo:warning=Skipping cliproxy build because ATM_SKIP_CLIPROXY_BUILD=1");
    }

    tauri_build::build()
}

fn build_cliproxy() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
    let repo_root = manifest_dir
        .parent()
        .expect("src-tauri should have a repository root parent")
        .to_path_buf();
    let script_path = repo_root.join("scripts").join("build-cliproxy.sh");
    let output_path = manifest_dir.join("resources").join("cliproxy-server");
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS is not set");
    let target_arch =
        env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH is not set");

    if !script_path.exists() {
        panic!(
            "cliproxy build script is missing: {}",
            script_path.display()
        );
    }

    let status = Command::new("bash")
        .arg(&script_path)
        .current_dir(&repo_root)
        .env("GOOS", map_target_os(&target_os))
        .env("GOARCH", map_target_arch(&target_arch))
        .env("OUTPUT_PATH", &output_path)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to start cliproxy build script {}: {}",
                script_path.display(),
                error
            )
        });

    if !status.success() {
        panic!("cliproxy build script failed with status: {}", status);
    }
}

fn map_target_os(target_os: &str) -> &str {
    match target_os {
        "macos" => "darwin",
        other => other,
    }
}

fn map_target_arch(target_arch: &str) -> &str {
    match target_arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}
