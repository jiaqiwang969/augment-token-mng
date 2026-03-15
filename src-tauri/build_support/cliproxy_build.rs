use std::fs;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSyncOutcome {
    Created,
    Updated,
    Unchanged,
}

pub fn replace_file_if_changed(source: &Path, destination: &Path) -> io::Result<FileSyncOutcome> {
    if !destination.exists() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, destination)?;
        return Ok(FileSyncOutcome::Created);
    }

    if files_have_same_content(source, destination)? {
        fs::remove_file(source)?;
        return Ok(FileSyncOutcome::Unchanged);
    }

    fs::copy(source, destination)?;
    fs::remove_file(source)?;
    Ok(FileSyncOutcome::Updated)
}

fn files_have_same_content(left: &Path, right: &Path) -> io::Result<bool> {
    let left_metadata = fs::metadata(left)?;
    let right_metadata = fs::metadata(right)?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }

    let mut left_file = fs::File::open(left)?;
    let mut right_file = fs::File::open(right)?;
    let mut left_buffer = [0_u8; 8192];
    let mut right_buffer = [0_u8; 8192];

    loop {
        let left_read = left_file.read(&mut left_buffer)?;
        let right_read = right_file.read(&mut right_buffer)?;
        if left_read != right_read {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
        if left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
    }
}
