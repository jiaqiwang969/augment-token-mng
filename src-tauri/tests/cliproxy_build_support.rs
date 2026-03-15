#[path = "../build_support/cliproxy_build.rs"]
mod cliproxy_build;

use std::thread::sleep;
use std::time::Duration;

use tempfile::tempdir;

use cliproxy_build::FileSyncOutcome;

#[test]
fn cliproxy_build_support_keeps_destination_when_binary_is_unchanged() {
    let temp_dir = tempdir().unwrap();
    let built_binary = temp_dir.path().join("cliproxy-server.new");
    let destination = temp_dir.path().join("cliproxy-server");

    std::fs::write(&built_binary, b"same-bytes").unwrap();
    std::fs::write(&destination, b"same-bytes").unwrap();

    let before = std::fs::metadata(&destination).unwrap().modified().unwrap();
    sleep(Duration::from_secs(1));

    let outcome = cliproxy_build::replace_file_if_changed(&built_binary, &destination).unwrap();

    let after = std::fs::metadata(&destination).unwrap().modified().unwrap();
    assert_eq!(outcome, FileSyncOutcome::Unchanged);
    assert_eq!(std::fs::read(&destination).unwrap(), b"same-bytes");
    assert_eq!(after, before);
}

#[test]
fn cliproxy_build_support_replaces_destination_when_binary_changes() {
    let temp_dir = tempdir().unwrap();
    let built_binary = temp_dir.path().join("cliproxy-server.new");
    let destination = temp_dir.path().join("cliproxy-server");

    std::fs::write(&built_binary, b"new-bytes").unwrap();
    std::fs::write(&destination, b"old-bytes").unwrap();

    let outcome = cliproxy_build::replace_file_if_changed(&built_binary, &destination).unwrap();

    assert_eq!(outcome, FileSyncOutcome::Updated);
    assert_eq!(std::fs::read(&destination).unwrap(), b"new-bytes");
}
