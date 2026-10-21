use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::error::VaultError;
use super::vault_file::parse_vault_file;

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

pub fn export(folder: &Path, destination: &Path) -> Result<(), VaultError> {
    let vault = folder.join(VAULT);
    let vault_bytes = fs::read(&vault).map_err(storage)?;
    let (vault_id, created_at) = match parse_vault_file(&vault_bytes) {
        Ok(parsed) => (
            parsed.header.vault_id.to_string(),
            parsed.header.created_at.clone(),
        ),
        Err(_) => (String::new(), String::new()),
    };
    let mut files = vec![(VAULT.to_string(), vault_bytes)];

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
            let bytes = fs::read(attachments.join(&name)).map_err(storage)?;
            files.push((format!("attachments/{name}"), bytes));
        }
    }

    let manifest = Manifest {
        format: "pmsvault".to_string(),
        version: 1,
        vault_id,
        created_at,
        exported_at: chrono::Utc::now().to_rfc3339(),
        files: files
            .iter()
            .map(|(path, bytes)| ManifestFile {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: sha256(bytes),
            })
            .collect(),
    };
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(|_| VaultError::ImportCorrupt)?;

    let output = File::create(destination).map_err(storage)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options)
        .map_err(|_| VaultError::ImportCorrupt)?;
    zip.write_all(&manifest_json).map_err(storage)?;
    for (path, bytes) in files {
        zip.start_file(path, options)
            .map_err(|_| VaultError::ImportCorrupt)?;
        zip.write_all(&bytes).map_err(storage)?;
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
    let cleanup = || {
        if created {
            let _ = fs::remove_dir_all(destination);
        }
    };
    let mut extracted = std::collections::HashSet::new();

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
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(storage)?;
        if bytes.len() as u64 != expected_file.size || sha256(&bytes) != expected_file.sha256 {
            cleanup();
            return Err(VaultError::ImportCorrupt);
        }
        let mut output = File::create(&target).map_err(storage)?;
        output.write_all(&bytes).map_err(storage)?;
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
