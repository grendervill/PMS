use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::Serialize;

use crate::commands::{ApiError, ApiResult};
use crate::core::ssh::{
    GeneratedSshKey, SshFormats, SshGenerateOptions, formats, generate,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSshKey {
    pub private_path: String,
    pub public_path: String,
    pub private_mode: u32,
    pub public_mode: u32,
    pub directory_mode: u32,
    pub owned_by_current_user: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshPermissionStatus {
    pub exists: bool,
    pub private_mode: Option<u32>,
    pub public_mode: Option<u32>,
    pub directory_mode: Option<u32>,
    pub owned_by_current_user: bool,
    pub ok: bool,
}

fn error(message: impl std::fmt::Display) -> ApiError {
    ApiError::new("ssh_storage", format!("Ошибка доступа к SSH-файлам: {message}"))
}

fn safe_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(ApiError::new(
            "invalid_ssh_name",
            "Имя файла может содержать только буквы, цифры, точку, дефис и подчёркивание",
        ));
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), ApiError> {
    let atomic = AtomicFile::new(path, AllowOverwrite);
    atomic
        .write(|file| {
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok::<(), std::io::Error>(())
        })
        .map_err(error)?;
    set_mode(path, mode)?;
    #[cfg(windows)]
    set_windows_acl(path)?;
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> Result<(), ApiError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(error)?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

#[cfg(windows)]
fn set_windows_acl(path: &Path) -> Result<(), ApiError> {
    use std::process::Command;
    let user = std::env::var("USERNAME").map_err(error)?;
    let status = Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("{user}:F")])
        .status()
        .map_err(error)?;
    if !status.success() {
        return Err(ApiError::new(
            "ssh_acl",
            "Не удалось выставить ACL для SSH-ключа",
        ));
    }
    Ok(())
}

fn mode_of(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .ok()
            .map(|metadata| metadata.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

fn current_user_owns(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        let uid = unsafe { libc::geteuid() };
        metadata.uid() == uid
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

#[tauri::command]
pub fn ssh_generate(options: SshGenerateOptions) -> ApiResult<GeneratedSshKey> {
    Ok(generate(&options)?)
}

#[tauri::command]
pub fn ssh_formats(
    private_key: String,
    passphrase: Option<String>,
) -> ApiResult<SshFormats> {
    Ok(formats(&private_key, passphrase.as_deref())?)
}

#[tauri::command]
pub fn ssh_install(
    directory: String,
    name: String,
    private_key: String,
    public_key: String,
    overwrite: bool,
) -> ApiResult<InstalledSshKey> {
    safe_name(&name)?;
    let directory_path = PathBuf::from(&directory);
    fs::create_dir_all(&directory_path).map_err(error)?;
    set_mode(&directory_path, 0o700)?;

    let private_path = directory_path.join(&name);
    let public_path = directory_path.join(format!("{name}.pub"));
    if !overwrite && (private_path.exists() || public_path.exists()) {
        return Err(ApiError::new(
            "file_exists",
            "Файл с таким именем уже существует",
        ));
    }

    write_atomic(&private_path, private_key.as_bytes(), 0o600)?;
    write_atomic(&public_path, public_key.as_bytes(), 0o644)?;

    let owned = current_user_owns(&private_path) && current_user_owns(&public_path);
    let warning = if !owned && cfg!(unix) {
        Some(format!(
            "Файлы принадлежат другому пользователю. Выполните: sudo chown -R $(id -u):$(id -g) {}",
            directory_path.display()
        ))
    } else {
        None
    };

    Ok(InstalledSshKey {
        private_path: private_path.to_string_lossy().into_owned(),
        public_path: public_path.to_string_lossy().into_owned(),
        private_mode: mode_of(&private_path).unwrap_or(0o600),
        public_mode: mode_of(&public_path).unwrap_or(0o644),
        directory_mode: mode_of(&directory_path).unwrap_or(0o700),
        owned_by_current_user: owned,
        warning,
    })
}

#[tauri::command]
pub fn ssh_check(directory: String, name: String) -> ApiResult<SshPermissionStatus> {
    safe_name(&name)?;
    let directory_path = Path::new(&directory);
    let private_path = directory_path.join(&name);
    let public_path = directory_path.join(format!("{name}.pub"));

    let exists = private_path.exists() && public_path.exists();
    let private_mode = mode_of(&private_path);
    let public_mode = mode_of(&public_path);
    let directory_mode = mode_of(directory_path);
    let owned = current_user_owns(&private_path) && current_user_owns(&public_path);
    let ok = if exists {
        #[cfg(unix)]
        {
            directory_mode == Some(0o700)
                && private_mode == Some(0o600)
                && public_mode == Some(0o644)
                && owned
        }
        #[cfg(not(unix))]
        {
            owned
        }
    } else {
        false
    };

    Ok(SshPermissionStatus {
        exists,
        private_mode,
        public_mode,
        directory_mode,
        owned_by_current_user: owned,
        ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ssh::{SshGenerateOptions, generate};
    use tempfile::tempdir;

    fn generated() -> (String, String) {
        let key = generate(&SshGenerateOptions::default()).unwrap();
        (key.private_key, key.public_key)
    }

    #[cfg(unix)]
    #[test]
    fn installs_with_private_permissions_and_checks_them() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (private_key, public_key) = generated();
        let installed = ssh_install(
            dir.path().to_string_lossy().into_owned(),
            "id_ed25519".to_string(),
            private_key,
            public_key,
            false,
        )
        .unwrap();

        assert_eq!(installed.private_mode, 0o600);
        assert_eq!(installed.public_mode, 0o644);
        assert_eq!(installed.directory_mode, 0o700);
        assert!(installed.owned_by_current_user);
        assert_eq!(
            fs::metadata(&installed.private_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let status = ssh_check(
            dir.path().to_string_lossy().into_owned(),
            "id_ed25519".to_string(),
        )
        .unwrap();
        assert!(status.ok);
    }

    #[test]
    fn rejects_path_traversal_names() {
        let dir = tempdir().unwrap();
        let (private_key, public_key) = generated();
        for name in ["../escape", "a/b", "", ".."] {
            assert_eq!(
                ssh_install(
                    dir.path().to_string_lossy().into_owned(),
                    name.to_string(),
                    private_key.clone(),
                    public_key.clone(),
                    false,
                )
                .unwrap_err()
                .code,
                "invalid_ssh_name"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_overwrite_without_flag() {
        let dir = tempdir().unwrap();
        let (private_key, public_key) = generated();
        let path = dir.path().to_string_lossy().into_owned();
        ssh_install(
            path.clone(),
            "id_ed25519".to_string(),
            private_key.clone(),
            public_key.clone(),
            false,
        )
        .unwrap();

        assert_eq!(
            ssh_install(path.clone(), "id_ed25519".to_string(), private_key, public_key, false)
                .unwrap_err()
                .code,
            "file_exists"
        );
    }

    #[cfg(unix)]
    #[test]
    fn check_reports_wrong_modes() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let (private_key, public_key) = generated();
        let path = dir.path().to_string_lossy().into_owned();
        let installed = ssh_install(
            path.clone(),
            "id_ed25519".to_string(),
            private_key,
            public_key,
            false,
        )
        .unwrap();
        fs::set_permissions(
            &installed.private_path,
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();

        let status = ssh_check(path, "id_ed25519".to_string()).unwrap();
        assert!(!status.ok);
        assert_eq!(status.private_mode, Some(0o644));
    }
}
