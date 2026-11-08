//! WhatsApp session snapshots, ported from `src/session-backup.mjs`.
//!
//! Chromium keeps the linked-device credentials in IndexedDB / Local Storage /
//! Cookies inside the userData directory. Snapshotting those directories
//! before Chromium opens them (i.e. at startup) gives the user a way back if a
//! forced kill corrupts LevelDB and WhatsApp asks for a QR scan again.

use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{NaiveDateTime, Utc};
use walkdir::WalkDir;

pub const ITEMS: [&str; 8] = [
    // Chromium (Electron) profile data.
    "IndexedDB",
    "Local Storage",
    "Session Storage",
    "Cookies",
    "Network",
    // WebKitGTK profile data used by the Rust/Tauri build. Keeping both in the
    // same snapshot means switching between the two front-ends never loses the
    // linked WhatsApp session.
    "webkit",
    "webkit-settings",
    // WebView2 profile used by the Windows Rust build.
    "webview2",
];
pub const KEEP: usize = 3;
/// Do not copy the multi-hundred-megabyte profile on every launch. A snapshot
/// from the last few hours is still a valid recovery point and keeps startup
/// fast when the app is restarted in quick succession.
pub const MIN_INTERVAL_HOURS: i64 = 6;
pub const FLAG: &str = "restore-session.flag";

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

fn backup_root(user_data: &Path) -> PathBuf {
    user_data.join("session-backup")
}

fn flag_path(user_data: &Path) -> PathBuf {
    user_data.join(FLAG)
}

pub fn stamp() -> String {
    Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string()
}

fn is_snapshot_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() != 19 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            4 | 7 | 13 | 16 => {
                if *byte != b'-' {
                    return false;
                }
            }
            10 => {
                if *byte != b'T' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_digit() {
                    return false;
                }
            }
        }
    }
    true
}

pub fn list_snapshots(user_data: &Path) -> Vec<String> {
    let root = backup_root(user_data);
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut snapshots: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_snapshot_name(name))
        .collect();
    snapshots.sort();
    snapshots
}

pub fn latest_snapshot_name(user_data: &Path) -> Option<String> {
    list_snapshots(user_data).pop()
}

/// Caches can be regenerated and are not needed to keep the WhatsApp session.
/// Skipping them makes snapshots smaller and startup faster.
fn is_cache_entry(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("WebKitCache" | "CacheStorage" | "serviceworkers")
        )
    })
}

fn copy_path(src: &Path, dst: &Path) -> Result<(), BackupError> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        return Ok(());
    }

    fs::create_dir_all(dst)?;

    let entries = WalkDir::new(src).into_iter().filter_entry(|entry| {
        let relative = entry.path().strip_prefix(src).unwrap_or(entry.path());
        !is_cache_entry(relative)
    });
    for entry in entries {
        let entry = entry?;
        let relative = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        } else if entry.file_type().is_symlink() {
            #[cfg(unix)]
            {
                let link = fs::read_link(entry.path())?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::remove_file(&target);
                std::os::unix::fs::symlink(link, &target)?;
            }
        }
    }

    Ok(())
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

fn prune_snapshots(user_data: &Path) {
    let root = backup_root(user_data);
    let snapshots = list_snapshots(user_data);
    let excess = snapshots.len().saturating_sub(KEEP);
    for old in snapshots.into_iter().take(excess) {
        let _ = remove_path(&root.join(old));
    }
}

/// Create a snapshot. Returns the snapshot path, or `None` when there is no
/// session data yet (first launch / freshly logged out profile) or when the
/// newest snapshot is still recent enough.
pub fn backup_session_data(user_data: &Path) -> Result<Option<PathBuf>, BackupError> {
    // Keep the disk bounded even when this launch skips the copy.
    prune_snapshots(user_data);

    if let Some(latest) = latest_snapshot_name(user_data) {
        if let Ok(stamp) = NaiveDateTime::parse_from_str(&latest, "%Y-%m-%dT%H-%M-%S") {
            let age = Utc::now().signed_duration_since(stamp.and_utc());
            if age.num_hours() < MIN_INTERVAL_HOURS {
                tracing::debug!(
                    snapshot = %latest,
                    age_minutes = age.num_minutes(),
                    "skipping session snapshot; recent snapshot exists"
                );
                return Ok(None);
            }
        }
    }

    let root = backup_root(user_data);
    let destination = root.join(stamp());
    let mut copied = 0usize;

    for item in ITEMS {
        let source = user_data.join(item);
        if !source.exists() {
            continue;
        }
        fs::create_dir_all(&destination)?;
        copy_path(&source, &destination.join(item))?;
        copied += 1;
    }

    if copied == 0 {
        let _ = remove_path(&destination);
        return Ok(None);
    }

    prune_snapshots(user_data);

    tracing::info!(
        snapshot = %destination.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        copied,
        "WhatsApp session snapshot created"
    );
    Ok(Some(destination))
}

/// Restore the latest snapshot when `restore-session.flag` exists. The current
/// (possibly corrupt) data is moved to `session-backup/rusak-<stamp>` instead
/// of being deleted, matching the Electron implementation.
pub fn restore_session_backup_if_requested(user_data: &Path) -> Result<bool, BackupError> {
    let flag = flag_path(user_data);
    if !flag.exists() {
        return Ok(false);
    }

    let result = (|| -> Result<bool, BackupError> {
        let Some(latest) = latest_snapshot_name(user_data) else {
            tracing::warn!("restore requested but no snapshot exists");
            return Ok(false);
        };

        let source_dir = backup_root(user_data).join(&latest);
        let broken = backup_root(user_data).join(format!("rusak-{}", stamp()));
        let mut moved = 0usize;

        for item in ITEMS {
            let current = user_data.join(item);
            if current.exists() {
                fs::create_dir_all(&broken)?;
                let target = broken.join(item);
                let _ = remove_path(&target);
                fs::rename(&current, &target)?;
                moved += 1;
            }
        }

        for entry in fs::read_dir(&source_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("rusak-") {
                continue;
            }
            copy_path(&entry.path(), &user_data.join(&name))?;
        }

        tracing::info!(
            snapshot = %latest,
            moved,
            broken = %broken.display(),
            "WhatsApp session restored from snapshot"
        );
        Ok(true)
    })();

    let _ = fs::remove_file(&flag);
    result
}

/// Write the restore flag so the next startup restores the newest snapshot.
pub fn request_session_restore(user_data: &Path) -> Result<Option<String>, BackupError> {
    let Some(latest) = latest_snapshot_name(user_data) else {
        return Ok(None);
    };

    fs::create_dir_all(user_data)?;
    fs::write(flag_path(user_data), &latest)?;
    Ok(Some(latest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_profile(root: &Path) {
        fs::create_dir_all(root.join("IndexedDB/leveldb")).unwrap();
        fs::write(root.join("IndexedDB/leveldb/000001.log"), b"db").unwrap();
        fs::create_dir_all(root.join("Local Storage/leveldb")).unwrap();
        fs::write(root.join("Local Storage/leveldb/000002.log"), b"ls").unwrap();
        fs::write(root.join("Cookies"), b"cookies").unwrap();
    }

    #[test]
    fn creates_and_restores_a_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path();
        seed_profile(profile);

        let snapshot = backup_session_data(profile).unwrap().expect("snapshot");
        assert!(snapshot.join("IndexedDB/leveldb/000001.log").exists());
        assert!(snapshot.join("Cookies").exists());
        assert_eq!(list_snapshots(profile).len(), 1);

        // Simulate corruption.
        fs::write(profile.join("IndexedDB/leveldb/000001.log"), b"broken").unwrap();
        assert!(request_session_restore(profile).unwrap().is_some());

        assert!(restore_session_backup_if_requested(profile).unwrap());
        assert_eq!(
            fs::read(profile.join("IndexedDB/leveldb/000001.log")).unwrap(),
            b"db"
        );

        // The corrupt copy is preserved, not deleted.
        let broken: Vec<_> = fs::read_dir(profile.join("session-backup"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("rusak-"))
            .collect();
        assert_eq!(broken.len(), 1);
    }

    #[test]
    fn no_data_means_no_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        assert!(backup_session_data(dir.path()).unwrap().is_none());
        assert!(list_snapshots(dir.path()).is_empty());
    }

    #[test]
    fn snapshot_names_are_validated() {
        assert!(is_snapshot_name("2026-11-07T12-34-56"));
        assert!(!is_snapshot_name("2026-11-07T12:34:56"));
        assert!(!is_snapshot_name("rusak-2026-11-07T12-34-56"));
    }

    #[test]
    fn keeps_only_recent_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("session-backup");
        for day in 1..=7 {
            fs::create_dir_all(root.join(format!("2026-11-0{day}T00-00-00"))).unwrap();
        }
        seed_profile(dir.path());
        backup_session_data(dir.path()).unwrap();

        let snapshots = list_snapshots(dir.path());
        assert_eq!(snapshots.len(), KEEP);
        assert!(!snapshots.contains(&"2026-11-01T00-00-00".to_string()));
    }

    #[test]
    fn skips_recent_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        seed_profile(dir.path());
        assert!(backup_session_data(dir.path()).unwrap().is_some());
        assert!(backup_session_data(dir.path()).unwrap().is_none());
        assert_eq!(list_snapshots(dir.path()).len(), 1);
    }

    #[test]
    fn snapshot_skips_regenerable_caches() {
        let dir = tempfile::tempdir().unwrap();
        seed_profile(dir.path());
        fs::create_dir_all(dir.path().join("webkit/WebKitCache/Version 17")).unwrap();
        fs::write(
            dir.path().join("webkit/WebKitCache/Version 17/blob"),
            b"cache",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("webkit/CacheStorage/cache")).unwrap();
        fs::write(dir.path().join("webkit/CacheStorage/cache/blob"), b"cache").unwrap();
        fs::create_dir_all(dir.path().join("webkit/localstorage")).unwrap();
        fs::write(dir.path().join("webkit/localstorage/state"), b"session").unwrap();

        let snapshot = backup_session_data(dir.path()).unwrap().expect("snapshot");
        assert!(snapshot.join("webkit/localstorage/state").exists());
        assert!(!snapshot.join("webkit/WebKitCache").exists());
        assert!(!snapshot.join("webkit/CacheStorage").exists());
    }
}
