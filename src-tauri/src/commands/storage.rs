use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;

use atomicwrites::{AllowOverwrite, AtomicFile};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;

use crate::commands::{ApiError, ApiResult};

const MAX_READ_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResponse {
    pub b64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryInfo {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ms: Option<u64>,
}

fn storage_error(message: impl std::fmt::Display) -> ApiError {
    ApiError::new("storage", format!("Ошибка доступа к файлу: {message}"))
}

fn decode_b64(value: &str) -> ApiResult<Vec<u8>> {
    STANDARD
        .decode(value)
        .map_err(|_| ApiError::bad_base64())
}

fn set_mode(path: &Path, mode: Option<u32>) -> ApiResult<()> {
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(storage_error)?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], mode: Option<u32>) -> ApiResult<()> {
    let atomic = AtomicFile::new(path, AllowOverwrite);
    atomic
        .write(|file| {
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok::<(), std::io::Error>(())
        })
        .map_err(storage_error)?;
    set_mode(path, mode)
}

fn read_file(path: &Path) -> ApiResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(storage_error)?;
    if metadata.len() > MAX_READ_BYTES {
        return Err(ApiError::new(
            "file_too_large",
            "Файл слишком большой для этой операции",
        ));
    }
    fs::read(path).map_err(storage_error)
}

fn stat(path: &Path) -> ApiResult<FileStat> {
    let metadata = fs::metadata(path).map_err(storage_error)?;
    Ok(FileStat {
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        mtime_ms: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64),
    })
}

#[tauri::command]
pub fn storage_read_file(path: String) -> ApiResult<ReadFileResponse> {
    let bytes = read_file(Path::new(&path))?;
    Ok(ReadFileResponse {
        b64: STANDARD.encode(bytes),
    })
}

#[tauri::command]
pub fn storage_write_atomic(
    path: String,
    b64: String,
    mode: Option<u32>,
) -> ApiResult<()> {
    let bytes = decode_b64(&b64)?;
    write_atomic(Path::new(&path), &bytes, mode)
}

#[tauri::command]
pub fn storage_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
pub fn storage_mkdir(path: String, mode: Option<u32>) -> ApiResult<()> {
    let path = Path::new(&path);
    let existed = path.exists();
    fs::create_dir_all(path).map_err(storage_error)?;
    if existed {
        return Ok(());
    }
    set_mode(path, mode)
}

#[tauri::command]
pub fn storage_stat(path: String) -> ApiResult<FileStat> {
    stat(Path::new(&path))
}

#[tauri::command]
pub fn storage_list_dir(path: String) -> ApiResult<Vec<DirEntryInfo>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(Path::new(&path)).map_err(storage_error)? {
        let entry = entry.map_err(storage_error)?;
        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(storage_error)?;
        entries.push(DirEntryInfo {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry_path.to_string_lossy().into_owned(),
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            mtime_ms: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[tauri::command]
pub fn storage_remove_file(path: String) -> ApiResult<()> {
    fs::remove_file(Path::new(&path)).map_err(storage_error)
}

#[tauri::command]
pub fn storage_remove_dir(path: String) -> ApiResult<()> {
    let path = Path::new(&path);
    if path.exists() {
        fs::remove_dir_all(path).map_err(storage_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_and_reads_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.bin");
        write_atomic(&path, b"hello", Some(0o600)).unwrap();

        assert_eq!(read_file(&path).unwrap(), b"hello");
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.bin");
        write_atomic(&path, b"first", None).unwrap();
        write_atomic(&path, b"second", None).unwrap();

        assert_eq!(read_file(&path).unwrap(), b"second");
    }

    #[cfg(unix)]
    #[test]
    fn sets_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.bin");
        write_atomic(&path, b"secret", Some(0o600)).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn failed_write_keeps_existing_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("vault.bin");
        fs::create_dir(&target).unwrap();

        assert!(write_atomic(&target, b"data", None).is_err());
        assert!(target.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn mkdir_does_not_chmod_an_existing_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let existing = dir.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();

        storage_mkdir(existing.to_string_lossy().into_owned(), Some(0o700)).unwrap();

        let mode = fs::metadata(&existing).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn stats_file_and_directory() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("vault.bin");
        write_atomic(&file, b"12345", None).unwrap();

        let file_stat = stat(&file).unwrap();
        assert!(file_stat.is_file);
        assert_eq!(file_stat.size, 5);
        assert!(file_stat.mtime_ms.is_some());

        let dir_stat = stat(dir.path()).unwrap();
        assert!(dir_stat.is_dir);
        assert!(!dir_stat.is_file);
    }

    #[test]
    fn lists_directory_sorted() {
        let dir = tempdir().unwrap();
        let b = dir.path().join("b.vault");
        let a = dir.path().join("a.vault");
        write_atomic(&a, b"a", None).unwrap();
        write_atomic(&b, b"b", None).unwrap();

        let entries = storage_list_dir(dir.path().to_string_lossy().into_owned()).unwrap();
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();

        assert_eq!(names, vec!["a.vault", "b.vault"]);
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");

        assert!(matches!(
            read_file(&missing),
            Err(ApiError { ref code, .. }) if code == "storage"
        ));
    }

    #[test]
    fn atomic_path_remains_original_type_on_failure() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("vault.bin");
        fs::write(&target, b"original").unwrap();

        assert!(write_atomic(&target, b"new", None).is_ok());
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }
}
