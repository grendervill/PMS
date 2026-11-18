use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::error::VaultError;
use super::models::VaultHeader;
use super::vault_file::{FIXED_HEADER_LEN, MAGIC, MAX_HEADER_LEN};

const MANIFEST: &str = "manifest.json";
const VAULT: &str = "pms.vault";
const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u32,
    vault_id: String,
    created_at: String,
    exported_at: String,
    files: Vec<ManifestFile>,
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn storage(error: impl std::fmt::Display) -> VaultError {
    VaultError::Storage(error.to_string())
}

/// Creates (or truncates) a file that only the current user can read. `umask`
/// narrows `open` modes, so the permission is also set explicitly afterwards.
fn create_private_file(path: &Path) -> Result<File, VaultError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(storage)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(storage)?;
    }
    Ok(file)
}

fn set_private_dir(path: &Path) -> Result<(), VaultError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(storage)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn parse_manifest(bytes: &[u8]) -> Result<Manifest, VaultError> {
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| VaultError::ImportCorrupt)?;
    if manifest.format != "pmsvault" || manifest.version != 1 {
        return Err(VaultError::ImportUnsupported);
    }
    Ok(manifest)
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path
            .split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty())
}

const COPY_BUFFER: usize = 1024 * 1024;

fn collect_files(folder: &Path) -> Result<Vec<(String, PathBuf, u64)>, VaultError> {
    let mut files = Vec::new();
    let vault = folder.join(VAULT);
    let size = fs::metadata(&vault).map_err(storage)?.len();
    files.push((VAULT.to_string(), vault, size));

    let attachments = folder.join("attachments");
    if attachments.is_dir() {
        let mut names = fs::read_dir(&attachments)
            .map_err(storage)?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        for name in names {
            if !name.ends_with(".pmsa") || !safe_path(&name) {
                continue;
            }
            let path = attachments.join(&name);
            let size = fs::metadata(&path).map_err(storage)?.len();
            files.push((format!("attachments/{name}"), path, size));
        }
    }
    Ok(files)
}

/// Hashes a file in fixed-size chunks. Nothing whole-file is ever held in
/// memory: a 2 GiB vault plus its attachments used to be read with `fs::read`
/// into one `Vec` per file and then again for the zip.
/// Copies `source` into `sink` while hashing it, capped by `declared`.
fn copy_and_hash<R: Read, W: Write>(
    source: &mut R,
    sink: &mut W,
    declared: u64,
) -> Result<(u64, String), VaultError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER];
    let mut total = 0u64;
    let mut limited = source.take(declared);
    loop {
        let read = limited.read(&mut buffer).map_err(storage)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
        sink.write_all(&buffer[..read]).map_err(storage)?;
    }
    if total != declared {
        return Err(VaultError::ImportCorrupt);
    }
    Ok((total, hex::encode(hasher.finalize())))
}

/// Upper bound for the hashing threads. Hashing is CPU-bound, but the pass
/// reads whole files from disk: more than a handful of readers makes a spinning
/// disk thrash, and past `available_parallelism` there is nothing to win.
const MAX_HASH_THREADS: usize = 8;

fn sha256_file(path: &Path) -> Result<(String, u64), VaultError> {
    let mut file = File::open(path).map_err(storage)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(storage)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Hashes every file of the manifest pass, in parallel.
///
/// The files are independent, so the work is split between a few threads that
/// pull indexes from a counter; each one writes its result into its own slot,
/// which keeps the output in `collect_files` order no matter who finishes first.
/// Nothing is collected as "the first error a thread ran into": the slots are
/// read back in index order, so the reported error is deterministic.
fn hash_files(files: &[(String, PathBuf, u64)]) -> Result<Vec<(String, u64)>, VaultError> {
    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(MAX_HASH_THREADS)
        .min(files.len())
        .max(1);

    type HashResult = Result<(String, u64), VaultError>;
    let slots: Mutex<Vec<Option<HashResult>>> =
        Mutex::new((0..files.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);

    // A single thread keeps the sequential behaviour of the old loop.
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= files.len() {
                    break;
                }
                let hashed = sha256_file(&files[index].1);
                // A panic can only happen inside `sha256_file`, and the scope
                // re-raises it, so a poisoned lock is not a real case here.
                let mut slots = slots.lock().unwrap_or_else(|poison| poison.into_inner());
                slots[index] = Some(hashed);
            });
        }
    });

    let slots = slots
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut hashes = Vec::with_capacity(files.len());
    for slot in slots {
        // Every index is written by construction: the threads only stop once the
        // counter passes the end of the list.
        hashes.push(slot.expect("every index is hashed")?);
    }
    Ok(hashes)
}

/// Builds the manifest entries in `files` order and checks that no file changed
/// size between `collect_files` and the hashing pass.
fn manifest_entries(
    files: &[(String, PathBuf, u64)],
    hashes: Vec<(String, u64)>,
) -> Result<Vec<ManifestFile>, VaultError> {
    let mut entries = Vec::with_capacity(files.len());
    for ((path, _real, size), (digest, actual)) in files.iter().zip(hashes) {
        if actual != *size {
            return Err(VaultError::Storage(format!(
                "размер файла {path} изменился во время экспорта"
            )));
        }
        entries.push(ManifestFile {
            path: path.clone(),
            size: *size,
            sha256: digest,
        });
    }
    Ok(entries)
}

/// Writes the manifest and then every file into `destination`, hashing the bytes
/// again while they are copied.
///
/// The two passes are deliberately separate: a file that changed between them
/// would leave an archive whose checksums never match, and the user would only
/// discover that when restoring. Detected here, the broken archive is removed.
fn write_archive(
    destination: &Path,
    files: &[(String, PathBuf, u64)],
    manifest: &Manifest,
) -> Result<(), VaultError> {
    let manifest_json =
        serde_json::to_vec_pretty(manifest).map_err(|_| VaultError::ImportCorrupt)?;

    let output = create_private_file(destination)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options)
        .map_err(|_| VaultError::ImportCorrupt)?;
    zip.write_all(&manifest_json).map_err(storage)?;

    let mut buffer = vec![0u8; COPY_BUFFER];
    let mut changed = None;
    for (index, (name, real, _)) in files.iter().enumerate() {
        zip.start_file(name.as_str(), options)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let mut file = File::open(real).map_err(storage)?;
        let mut hasher = Sha256::new();
        loop {
            let read = file.read(&mut buffer).map_err(storage)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            zip.write_all(&buffer[..read]).map_err(storage)?;
        }
        let digest = hex::encode(hasher.finalize());
        if manifest.files[index].sha256 != digest {
            changed = Some(name.clone());
            break;
        }
    }
    if let Some(name) = changed {
        drop(zip);
        let _ = fs::remove_file(destination);
        return Err(VaultError::Storage(format!(
            "файл {name} изменился во время экспорта, архив удалён — повторите"
        )));
    }
    zip.finish().map_err(storage)?;
    Ok(())
}

/// Reads just the header of `pms.vault` (magic, lengths, header JSON) instead of
/// the whole file, so the manifest can be built without loading gigabytes.
fn read_vault_identity(folder: &Path) -> Result<(String, String), VaultError> {
    let empty = (String::new(), String::new());
    let mut file = match File::open(folder.join(VAULT)) {
        Ok(file) => file,
        Err(_) => return Ok(empty),
    };
    let mut fixed = [0u8; FIXED_HEADER_LEN];
    if file.read_exact(&mut fixed).is_err() {
        return Ok(empty);
    }
    if &fixed[..5] != MAGIC {
        return Ok(empty);
    }
    let header_len = u32::from_le_bytes([fixed[5], fixed[6], fixed[7], fixed[8]]) as usize;
    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Ok(empty);
    }
    let mut header_json = vec![0u8; header_len];
    if file.read_exact(&mut header_json).is_err() {
        return Ok(empty);
    }
    match serde_json::from_slice::<VaultHeader>(&header_json) {
        Ok(header) => Ok((header.vault_id.to_string(), header.created_at)),
        Err(_) => Ok(empty),
    }
}

pub fn export(folder: &Path, destination: &Path) -> Result<(), VaultError> {
    let files = collect_files(folder)?;

    let (vault_id, created_at) = read_vault_identity(folder)?;

    // First pass: hash every file. This is independent CPU work per file, so it
    // runs on a few threads (the sizes were recorded before the pass started).
    let manifest_files = manifest_entries(&files, hash_files(&files)?)?;

    let manifest = Manifest {
        format: "pmsvault".to_string(),
        version: 1,
        vault_id,
        created_at,
        exported_at: chrono::Utc::now().to_rfc3339(),
        files: manifest_files,
    };

    // Second pass: copy the bytes and hash them again, so a file that changed in
    // between is detected instead of producing an archive that never restores.
    write_archive(destination, &files, &manifest)
}

pub fn import(archive_path: &Path, destination: &Path) -> Result<(), VaultError> {
    if destination.exists() {
        let has_entries = fs::read_dir(destination).map_err(storage)?.next().is_some();
        if has_entries {
            return Err(VaultError::ImportDestinationNotEmpty);
        }
    }

    let archive_size = fs::metadata(archive_path).map_err(storage)?.len();
    let file = File::open(archive_path).map_err(storage)?;
    let mut archive = ZipArchive::new(file).map_err(|_| VaultError::ImportCorrupt)?;

    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let name = entry
            .name()
            .map_err(|_| VaultError::ImportCorrupt)?
            .to_string();
        if name == MANIFEST {
            continue;
        }
        if !safe_path(&name) || !(name == VAULT || name.starts_with("attachments/")) {
            return Err(VaultError::ImportCorrupt);
        }
        total = total.saturating_add(entry.size());
    }
    if total > MAX_UNPACKED || total > archive_size.saturating_mul(2).max(1024 * 1024) {
        return Err(VaultError::ImportCorrupt);
    }

    let manifest_bytes = {
        let mut entry = archive
            .by_name(MANIFEST)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|_| VaultError::ImportCorrupt)?;
        bytes
    };
    let manifest = parse_manifest(&manifest_bytes)?;
    let expected = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<std::collections::HashMap<_, _>>();
    if !expected.contains_key(VAULT) {
        return Err(VaultError::ImportCorrupt);
    }

    let created = !destination.exists();
    fs::create_dir_all(destination).map_err(storage)?;
    set_private_dir(destination)?;
    let cleanup = || {
        if created {
            let _ = fs::remove_dir_all(destination);
        }
    };
    let mut extracted = std::collections::HashSet::new();
    let mut budget = crate::core::import::UnpackBudget::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let name = entry
            .name()
            .map_err(|_| VaultError::ImportCorrupt)?
            .to_string();
        if name == MANIFEST {
            continue;
        }
        let Some(expected_file) = expected.get(&name) else {
            cleanup();
            return Err(VaultError::ImportCorrupt);
        };
        let target = destination.join(&name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(storage)?;
            set_private_dir(parent)?;
        }
        let declared = entry.size();
        if !budget.take(declared)? {
            cleanup();
            return Err(VaultError::ImportCorrupt);
        }
        let mut output = create_private_file(&target)?;
        let (written, digest) = copy_and_hash(&mut entry, &mut output, declared)?;
        if written != expected_file.size || digest != expected_file.sha256 {
            cleanup();
            return Err(VaultError::ImportCorrupt);
        }
        extracted.insert(name);
    }

    if expected.keys().any(|path| !extracted.contains(path)) {
        cleanup();
        return Err(VaultError::ImportCorrupt);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn export_import_roundtrip() {
        let source = tempdir().unwrap();
        fs::write(source.path().join(VAULT), b"encrypted-vault").unwrap();
        fs::create_dir(source.path().join("attachments")).unwrap();
        fs::write(source.path().join("attachments/a.pmsa"), b"attachment").unwrap();
        let archive = source.path().join("backup.pmsvault");
        export(source.path(), &archive).unwrap();

        let target = tempdir().unwrap();
        let destination = target.path().join("restored");
        import(&archive, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join(VAULT)).unwrap(),
            b"encrypted-vault"
        );
        assert_eq!(
            fs::read(destination.join("attachments/a.pmsa")).unwrap(),
            b"attachment"
        );
    }

    fn read_manifest(archive: &Path) -> Manifest {
        let file = File::open(archive).unwrap();
        let mut zip = ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name(MANIFEST).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn attachment_bytes(index: u8) -> Vec<u8> {
        (0..256 * 1024)
            .map(|offset| (offset % 251) as u8 ^ index)
            .collect()
    }

    /// The hashing pass runs on several threads; the manifest must still list
    /// every file in `collect_files` order with the right digest and size, and
    /// the archive must restore byte for byte.
    #[test]
    fn export_hashes_every_file_and_keeps_the_manifest_order() {
        let source = tempdir().unwrap();
        fs::write(source.path().join(VAULT), b"encrypted-vault").unwrap();
        let attachments = source.path().join("attachments");
        fs::create_dir(&attachments).unwrap();
        let mut expected = Vec::new();
        for index in 0..12u8 {
            let name = format!("attachment-{index:02}.pmsa");
            let bytes = attachment_bytes(index);
            fs::write(attachments.join(&name), &bytes).unwrap();
            expected.push((format!("attachments/{name}"), bytes));
        }

        let archive = source.path().join("backup.pmsvault");
        export(source.path(), &archive).unwrap();

        let manifest = read_manifest(&archive);
        assert_eq!(manifest.files.len(), expected.len() + 1);
        let mut paths = vec![VAULT.to_string()];
        paths.extend(expected.iter().map(|(path, _)| path.clone()));
        assert_eq!(
            manifest
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            paths
        );

        let vault = &manifest.files[0];
        assert_eq!(vault.size, b"encrypted-vault".len() as u64);
        assert_eq!(vault.sha256, sha256(b"encrypted-vault"));
        for (entry, (_, bytes)) in manifest.files.iter().skip(1).zip(&expected) {
            assert_eq!(entry.size, bytes.len() as u64, "size of {}", entry.path);
            assert_eq!(entry.sha256, sha256(bytes), "digest of {}", entry.path);
        }

        let target = tempdir().unwrap();
        let destination = target.path().join("restored");
        import(&archive, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join(VAULT)).unwrap(),
            b"encrypted-vault"
        );
        for (path, bytes) in expected {
            assert_eq!(fs::read(destination.join(&path)).unwrap(), bytes, "{path}");
        }
    }

    /// A file that grew between `collect_files` and the hashing pass is caught by
    /// the size check, and the *first* such file is reported even though the
    /// threads finish in an arbitrary order.
    #[test]
    fn a_file_that_changed_size_during_hashing_is_reported_in_order() {
        let source = tempdir().unwrap();
        fs::write(source.path().join(VAULT), b"vault").unwrap();
        let attachments = source.path().join("attachments");
        fs::create_dir(&attachments).unwrap();
        for name in ["a.pmsa", "b.pmsa", "c.pmsa"] {
            fs::write(attachments.join(name), b"x").unwrap();
        }

        let files = collect_files(source.path()).unwrap();
        // Changed *after* the sizes were recorded, like a file being rewritten
        // while the export runs. `files[0]` is `pms.vault`.
        fs::write(&files[2].1, b"changed").unwrap();
        fs::write(&files[3].1, b"changed too").unwrap();
        let hashes = hash_files(&files).unwrap();

        let error = manifest_entries(&files, hashes).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("attachments/b.pmsa"),
            "the lowest index must win, got {message}"
        );
        assert!(!message.contains("c.pmsa"), "got {message}");
    }

    /// The second pass hashes the bytes again: a file rewritten after the
    /// manifest was built must fail the export and take the archive with it.
    #[test]
    fn a_file_changed_between_the_passes_removes_the_archive() {
        let source = tempdir().unwrap();
        fs::write(source.path().join(VAULT), b"vault").unwrap();
        let attachments = source.path().join("attachments");
        fs::create_dir(&attachments).unwrap();
        fs::write(attachments.join("a.pmsa"), b"first").unwrap();

        let files = collect_files(source.path()).unwrap();
        let manifest = Manifest {
            format: "pmsvault".to_string(),
            version: 1,
            vault_id: String::new(),
            created_at: String::new(),
            exported_at: String::new(),
            files: manifest_entries(&files, hash_files(&files).unwrap()).unwrap(),
        };
        // Exactly the race the second pass exists for.
        fs::write(attachments.join("a.pmsa"), b"second").unwrap();

        let archive_dir = tempdir().unwrap();
        let archive = archive_dir.path().join("backup.pmsvault");
        let error = write_archive(&archive, &files, &manifest).unwrap_err();
        assert!(
            error.to_string().contains("изменился"),
            "unexpected error: {error}"
        );
        assert!(!archive.exists(), "the broken archive must be removed");
    }

    #[test]
    fn refuses_non_empty_destination() {
        let source = tempdir().unwrap();
        fs::write(source.path().join(VAULT), b"vault").unwrap();
        let archive = source.path().join("backup.pmsvault");
        export(source.path(), &archive).unwrap();
        let target = tempdir().unwrap();
        fs::write(target.path().join("keep.txt"), b"keep").unwrap();
        assert!(matches!(
            import(&archive, target.path()),
            Err(VaultError::ImportDestinationNotEmpty)
        ));
        assert!(target.path().join("keep.txt").exists());
    }

    #[test]
    fn rejects_traversal_entry() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let output = file.reopen().unwrap();
        let mut zip = ZipWriter::new(output);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("../escape", options).unwrap();
        zip.write_all(b"bad").unwrap();
        zip.start_file(MANIFEST, options).unwrap();
        zip.write_all(br#"{"format":"pmsvault","version":1,"vaultId":"","createdAt":"","exportedAt":"","files":[{"path":"pms.vault","size":0,"sha256":""}]}"#)
            .unwrap();
        zip.finish().unwrap();
        let target = tempdir().unwrap();
        assert!(matches!(
            import(file.path(), &target.path().join("restored")),
            Err(VaultError::ImportCorrupt)
        ));
    }
}
