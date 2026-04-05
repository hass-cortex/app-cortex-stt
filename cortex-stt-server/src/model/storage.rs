use std::path::Path;

/// Recursively compute the total size (in bytes) of a file or directory.
pub fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }

    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total += dir_size(&entry_path);
            } else {
                total += entry_path.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// Return the free disk space (in bytes) for the filesystem containing `path`.
///
/// On Linux, reads from `/proc/mounts` and `statvfs`-like info via `std::fs`.
/// Falls back to 0 on unsupported platforms or errors.
pub fn free_disk_space(path: &Path) -> u64 {
    // Use the nix-independent approach: read /proc/self/statfs or
    // simply use std::process::Command to call `df`.
    // For simplicity, shell out to `df` which is universally available on Linux.
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("df")
            .arg("--output=avail")
            .arg("-B1")
            .arg(path.as_os_str())
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Output has a header line ("Avail") followed by the value.
                if let Some(line) = stdout.lines().nth(1) {
                    return line.trim().parse::<u64>().unwrap_or(0);
                }
            }
        }
        0
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(tmp.path()), 0);
    }

    #[test]
    fn dir_size_with_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").unwrap();
        assert_eq!(dir_size(tmp.path()), 11);
    }

    #[test]
    fn dir_size_single_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.bin");
        std::fs::write(&file_path, b"abcdef").unwrap();
        assert_eq!(dir_size(&file_path), 6);
    }

    #[test]
    fn dir_size_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.bin"), b"aaa").unwrap();
        std::fs::write(tmp.path().join("b.bin"), b"bb").unwrap();
        assert_eq!(dir_size(tmp.path()), 5);
    }

    #[test]
    fn free_disk_space_returns_nonzero_on_linux() {
        let tmp = tempfile::tempdir().unwrap();
        // On Linux CI this should be > 0; on other platforms it returns 0.
        let free = free_disk_space(tmp.path());
        if cfg!(target_os = "linux") {
            assert!(free > 0, "expected non-zero free space on Linux");
        }
    }
}
