//! On-disk storage for clipboard files.
//!
//! File contents never touch SQLite (see the migration for why). Each upload is
//! streamed to `<dir>/<user_id>/<uuid>.part` in fixed-size chunks, fsynced, then
//! atomically renamed to `<dir>/<user_id>/<uuid>`. Only after that rename does a
//! metadata row appear, so a crashed upload can never be visible as a file —
//! it leaves a `.part` that the retention sweep collects later.

use std::io;
use std::path::{Path, PathBuf};

use axum::extract::multipart::Field;
use tokio::io::AsyncWriteExt;

/// How often, in bytes written, to re-check free disk space mid-stream.
/// `Content-Length` is attacker-controlled, so the guard has to run during the
/// write rather than only before it.
const FREE_SPACE_CHECK_INTERVAL: u64 = 64 * 1024 * 1024;

/// Longest `original_name` we keep. Names are display-only, but they end up in
/// a `Content-Disposition` header, so they stay bounded.
pub const MAX_NAME_LEN: usize = 255;

/// Per-user upload directory.
pub fn user_dir(base: &str, user_id: i64) -> PathBuf {
    Path::new(base).join(user_id.to_string())
}

/// Absolute path of a stored file.
///
/// `stored_name` is always a UUID this app generated, never user input, so this
/// cannot be steered outside `base` — but callers still look the name up by
/// `(id, user_id)` before calling here.
pub fn file_path(base: &str, user_id: i64, stored_name: &str) -> PathBuf {
    user_dir(base, user_id).join(stored_name)
}

/// Free bytes available to an unprivileged user on the filesystem holding `path`.
///
/// Returns `None` when the path cannot be stat'd, in which case callers treat
/// free space as unknown and let the write proceed — the per-user quota is
/// still enforced.
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the call,
    // and `stat` is a properly sized, zeroed `statvfs` we own exclusively.
    let stat = unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        stat
    };
    Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

/// Verify at startup that the storage directory exists and is writable.
///
/// Worth doing eagerly: the failure modes here are configuration, not code, and
/// they only show up when someone tries to upload. EROFS in particular is easy
/// to misread as a permissions problem — under `ProtectSystem=strict` systemd
/// mounts everything outside `ReadWritePaths` read-only for the service, so the
/// directory can be owned by the right user, be `chmod 777`, and still reject
/// writes from the server while accepting them from a shell.
pub async fn check_writable(base: &str) -> Result<(), io::Error> {
    tokio::fs::create_dir_all(base).await?;
    let probe = Path::new(base).join(".write-probe");
    tokio::fs::write(&probe, b"").await?;
    let _ = tokio::fs::remove_file(&probe).await;
    Ok(())
}

/// Turn a storage-directory failure into a message naming the likely fix.
///
/// Split out from the logging so the wording for each case is testable.
pub fn writability_hint(e: &io::Error, base: &str) -> String {
    // EROFS(30) from a service that can write elsewhere almost always means the
    // systemd sandbox rather than a genuinely read-only disk.
    if e.raw_os_error() == Some(30) {
        format!(
            "FileClipboard: {base} is read-only for this process. If it is on a \
             separate disk, add it to ReadWritePaths in the systemd unit — \
             ProtectSystem=strict makes every other path read-only however the \
             directory is owned, so chown/chmod will not help — and check the \
             disk itself is not mounted ro. Uploads will fail until then."
        )
    } else {
        format!(
            "FileClipboard: cannot write to {base}: {e}. Uploads will fail until this is fixed."
        )
    }
}

/// Log the outcome of `check_writable`, translating the error into the fix.
pub async fn report_writability(base: &str) {
    match check_writable(base).await {
        Ok(()) => tracing::info!("FileClipboard: storage directory {base} is writable"),
        Err(e) => tracing::error!("{}", writability_hint(&e, base)),
    }
}

/// Limits applied to a single streamed upload.
pub struct Limits {
    /// Largest single file accepted.
    pub max_file_bytes: u64,
    /// Bytes this user may still store before hitting their quota.
    pub remaining_quota_bytes: u64,
    /// Abort if free space would fall below this.
    pub min_free_bytes: u64,
}

#[derive(Debug)]
pub enum UploadError {
    Empty,
    TooLarge,
    QuotaExceeded,
    DiskFull,
    Read(String),
    Io(io::Error),
}

pub struct Stored {
    pub stored_name: String,
    pub size_bytes: u64,
}

/// Stream one multipart field to disk, enforcing `limits` as the bytes arrive.
///
/// On any error the partial file is removed before returning, so a rejected
/// upload leaves nothing behind.
pub async fn store_field(
    base: &str,
    user_id: i64,
    field: &mut Field<'_>,
    limits: &Limits,
) -> Result<Stored, UploadError> {
    let dir = user_dir(base, user_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(UploadError::Io)?;

    let stored_name = uuid::Uuid::new_v4().to_string();
    let part_path = dir.join(format!("{stored_name}.part"));
    let final_path = dir.join(&stored_name);

    match write_stream(&part_path, &dir, field, limits).await {
        Ok(0) => {
            let _ = tokio::fs::remove_file(&part_path).await;
            Err(UploadError::Empty)
        }
        Ok(size_bytes) => {
            tokio::fs::rename(&part_path, &final_path)
                .await
                .map_err(UploadError::Io)?;
            Ok(Stored {
                stored_name,
                size_bytes,
            })
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&part_path).await;
            Err(e)
        }
    }
}

/// Chunked write loop. Never holds more than one chunk in memory, so a 5 GB
/// upload costs the same RAM as a 5 KB one.
async fn write_stream(
    part_path: &Path,
    dir: &Path,
    field: &mut Field<'_>,
    limits: &Limits,
) -> Result<u64, UploadError> {
    let file = tokio::fs::File::create(part_path)
        .await
        .map_err(UploadError::Io)?;
    let mut writer = tokio::io::BufWriter::new(file);

    let mut written: u64 = 0;
    let mut next_free_check: u64 = 0;

    loop {
        let chunk = match field.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(e) => return Err(UploadError::Read(e.to_string())),
        };

        written = written.saturating_add(chunk.len() as u64);
        if written > limits.max_file_bytes {
            return Err(UploadError::TooLarge);
        }
        if written > limits.remaining_quota_bytes {
            return Err(UploadError::QuotaExceeded);
        }
        if limits.min_free_bytes > 0 && written >= next_free_check {
            if let Some(free) = free_bytes(dir)
                && free < limits.min_free_bytes
            {
                return Err(UploadError::DiskFull);
            }
            next_free_check = written.saturating_add(FREE_SPACE_CHECK_INTERVAL);
        }

        writer.write_all(&chunk).await.map_err(UploadError::Io)?;
    }

    writer.flush().await.map_err(UploadError::Io)?;
    // fsync before the rename: the metadata row is written only once the bytes
    // are durable, so a power cut cannot leave a row pointing at a truncated file.
    writer
        .into_inner()
        .sync_all()
        .await
        .map_err(UploadError::Io)?;

    Ok(written)
}

/// Delete one stored file, ignoring a file that is already gone.
pub async fn remove(base: &str, user_id: i64, stored_name: &str) {
    let path = file_path(base, user_id, stored_name);
    if let Err(e) = tokio::fs::remove_file(&path).await
        && e.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!("FileClipboard: failed to delete {}: {e}", path.display());
    }
}

/// Strip a client-supplied filename down to a safe display name.
///
/// Takes the basename (a browser may send a full path), drops control
/// characters — they would otherwise be injected into a `Content-Disposition`
/// header — and bounds the length.
pub fn sanitize_name(raw: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches('.');

    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_LEN)
        .collect();

    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

/// Human-readable byte count for the file list.
pub fn fmt_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_takes_basename() {
        assert_eq!(sanitize_name("/etc/passwd"), "passwd");
        assert_eq!(sanitize_name(r"C:\Users\me\notes.txt"), "notes.txt");
        assert_eq!(sanitize_name("../../secret"), "secret");
    }

    #[test]
    fn sanitize_name_strips_control_characters() {
        assert_eq!(
            sanitize_name("evil\r\nX-Injected: 1.txt"),
            "evilX-Injected: 1.txt"
        );
    }

    #[test]
    fn sanitize_name_falls_back_when_empty() {
        assert_eq!(sanitize_name("   "), "unnamed");
        assert_eq!(sanitize_name("..."), "unnamed");
    }

    #[test]
    fn read_only_filesystem_points_at_the_systemd_sandbox() {
        // The exact error a service under ProtectSystem=strict gets for a
        // storage directory missing from ReadWritePaths.
        let erofs = io::Error::from_raw_os_error(30);
        let hint = writability_hint(&erofs, "/mnt/hdd/myapps");
        assert!(hint.contains("/mnt/hdd/myapps"));
        assert!(hint.contains("ReadWritePaths"), "must name the fix: {hint}");
        assert!(
            hint.contains("chown"),
            "must rule out the wrong fix: {hint}"
        );
    }

    #[test]
    fn other_errors_report_themselves_plainly() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        let hint = writability_hint(&denied, "/srv/files");
        assert!(hint.contains("/srv/files"));
        assert!(!hint.contains("ReadWritePaths"));
    }

    #[tokio::test]
    async fn check_writable_accepts_a_normal_directory() {
        let dir = std::env::temp_dir().join(format!("fc-probe-{}", uuid::Uuid::new_v4()));
        check_writable(dir.to_string_lossy().as_ref())
            .await
            .unwrap();
        assert!(dir.exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn check_writable_rejects_an_unwritable_location() {
        // /proc is not a place a directory can be created.
        let err = check_writable("/proc/myapps-file-clipboard-probe").await;
        assert!(err.is_err());
    }

    #[test]
    fn fmt_size_scales() {
        assert_eq!(fmt_size(512), "512 B");
        assert_eq!(fmt_size(2048), "2.0 KB");
        assert_eq!(fmt_size(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
