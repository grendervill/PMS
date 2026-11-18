use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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
    let header_len =
        u32::from_le_bytes([fixed[5], fixed[6], fixed[7], fixed[8]]) as usize;
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

    let mut manifest_files = Vec::with_capacity(files.len());
    for (path, real, size) in &files {
        let (digest, actual) = sha256_file(real)?;
        if actual != *size {
            return Err(VaultError::Storage(format!(
                "размер файла {path} изменился во время экспорта"
            )));
        }
        manifest_files.push(ManifestFile {
            path: path.clone(),
            size: *size,
            sha256: digest,
        });
    }

    let manifest = Manifest {
        format: "pmsvault".to_string(),
        version: 1,
        vault_id,
        created_at,
        exported_at: chrono::Utc::now().to_rfc3339(),
        files: manifest_files,
    };
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(|_| VaultError::ImportCorrupt)?;

    let output = create_private_file(destination)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options)
        .map_err(|_| VaultError::ImportCorrupt)?;
    zip.write_all(&manifest_json).map_err(storage)?;

    // Second pass: copy the bytes and hash them again. Hashing and copying are
    // separate passes (the manifest is written first), so a file that changed in
    // between would leave an archive whose checksums never match — the user
    // would only discover that when restoring. Detect it here instead.
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
