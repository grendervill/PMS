//! Filesystem scope for every command that takes a path.
//!
//! The webview can only reach paths the user explicitly granted through a
//! native dialog. Without this, a compromised renderer (a WebView 0-day, a
//! malicious npm package, a future HTML-injection bug) could call
//! `storage_write_atomic` with `~/.ssh/authorized_keys` and get code execution
//! with the user's privileges.
//!
//! Rules:
//! * a grant is added only when the user picks a folder/file in a *native*
//!   dialog (see [`super::dialogs`]); the renderer cannot grant itself paths;
//! * every path is canonicalized (symlinks resolved) and must be inside a
//!   grant; `..` components are rejected outright;
//! * the app cache directory is always readable and writable: archives and
//!   transfers are staged there;
//! * only *vault folders* are remembered across restarts. The grant store lives
//!   outside every grant and is re-validated on load, and a remembered grant is
//!   accepted only if the folder still contains a vault — so even a forged store
//!   cannot hand out access to something like `~/.ssh`.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::commands::{ApiError, ApiResult};

const MAX_GRANTS: usize = 32;
const MAX_PATH_LEN: usize = 4096;
/// A folder is remembered between restarts only if it looks like a vault.
const VAULT_FILE: &str = "pms.vault";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}

impl Access {
    pub fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub fn allows_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    fn merge(self, other: Access) -> Access {
        if self == other {
            self
        } else {
            Access::ReadWrite
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Grant {
    path: PathBuf,
    access: Access,
    /// Written to the grant store and therefore restored on the next launch.
    #[serde(default)]
    persisted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredScope {
    #[serde(default)]
    grants: Vec<Grant>,
}

#[derive(Default)]
struct ScopeState {
    grants: Vec<Grant>,
    store: Option<PathBuf>,
}

#[derive(Default)]
pub struct FsScope {
    inner: Mutex<ScopeState>,
}

fn scope_error() -> ApiError {
    ApiError::new(
        "fs_scope",
        "Нет доступа к этому пути: выберите папку или файл в диалоге",
    )
}

fn poisoned() -> ApiError {
    ApiError::new("fs_scope", "Внутренняя ошибка доступа к файлам")
}

/// Canonicalizes a path that may not exist yet.
///
/// `..` and `.` components are rejected instead of being normalized: every path
/// the app builds is absolute and free of them, so their presence means the
/// caller is trying to escape, not that the path is unusual.
fn normalize_candidate(raw: &str) -> ApiResult<PathBuf> {
    if raw.is_empty() || raw.len() > MAX_PATH_LEN || raw.contains('\0') {
        return Err(scope_error());
    }
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(scope_error());
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(scope_error());
    }
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }

    // Non-existent target (a file that is about to be written): canonicalize
    // the closest existing ancestor and re-attach the remaining names.
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path;
    loop {
        let Some(parent) = current.parent() else {
            return Err(scope_error());
        };
        let Some(name) = current.file_name() else {
            return Err(scope_error());
        };
        suffix.push(name.to_os_string());
        if suffix.len() > 16 {
            return Err(scope_error());
        }
        if let Ok(canonical) = parent.canonicalize() {
            let mut resolved = canonical;
            for name in suffix.iter().rev() {
                resolved.push(name);
            }
            return Ok(resolved);
        }
        current = parent;
    }
}

/// A folder is worth remembering when it still holds a vault: a forged store
/// then cannot grant access to an unrelated directory.
fn looks_like_vault_folder(path: &Path) -> bool {
    path.is_dir() && path.join(VAULT_FILE).is_file()
}

fn is_home(path: &Path) -> bool {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .and_then(|home| home.canonicalize().ok())
        .is_some_and(|home| home == path)
}

/// Re-validates a grant loaded from disk.
fn validate_loaded_grant(
    grant: Grant,
    store: Option<&Path>,
    always: &[PathBuf],
) -> Option<Grant> {
    if !grant.persisted {
        return None;
    }
    let canonical = grant.path.canonicalize().ok()?;
    if canonical.parent().is_none() || is_home(&canonical) {
        return None;
    }
    if !looks_like_vault_folder(&canonical) {
        return None;
    }
    let covers = |target: &Path| target.starts_with(&canonical);
    if store.is_some_and(covers) || always.iter().any(|dir| covers(dir)) {
        return None;
    }
    Some(Grant {
        path: canonical,
        access: grant.access,
        persisted: true,
    })
}

impl FsScope {
    /// Loads remembered grants and remembers where to save new ones. `always`
    /// lists paths the app itself owns (its cache directory).
    pub fn attach(&self, store: Option<PathBuf>, always: Vec<PathBuf>) {
        let store_canonical = store.as_deref().and_then(|path| {
            let parent = path.parent()?.canonicalize().ok()?;
            path.file_name().map(|name| parent.join(name))
        });
        let mut always_roots = Vec::new();
        for path in always {
            if let Ok(canonical) = path.canonicalize() {
                always_roots.push(canonical);
            }
        }

        let mut loaded = Vec::new();
        if let Some(path) = store.as_deref() {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(stored) = serde_json::from_slice::<StoredScope>(&bytes) {
                    for grant in stored.grants {
                        if let Some(grant) =
                            validate_loaded_grant(grant, store_canonical.as_deref(), &always_roots)
                        {
                            loaded.push(grant);
                        }
                    }
                }
            }
        }
        for canonical in always_roots {
            loaded.push(Grant {
                path: canonical,
                access: Access::ReadWrite,
                persisted: false,
            });
        }

        if let Ok(mut state) = self.inner.lock() {
            state.grants = dedup(loaded);
            state.store = store;
        }
    }

    /// Registers a path the user picked in a native dialog.
    pub fn grant(&self, path: &Path, access: Access) -> ApiResult<PathBuf> {
        let canonical = path.canonicalize().map_err(|_| scope_error())?;
        if canonical.parent().is_none() || is_home(&canonical) {
            return Err(scope_error());
        }
        self.insert(canonical, access, false)
    }

    /// Registers a save-dialog target that does not exist yet.
    ///
    /// `canonicalize` cannot be used on a missing file, so the closest existing
    /// ancestor is canonicalized and the remaining names are re-attached. Only
    /// the target itself is granted (the temporary file of an atomic write is
    /// created by the operating system, not by a scoped command), and it is not
    /// remembered after a restart.
    pub fn grant_target(&self, path: &Path, access: Access) -> ApiResult<PathBuf> {
        let resolved = normalize_candidate(&path.to_string_lossy())?;
        let parent = resolved
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(scope_error)?;
        if parent.parent().is_none() || !parent.is_dir() || is_home(&parent) {
            return Err(scope_error());
        }
        self.insert(resolved, access, false)
    }

    fn insert(&self, path: PathBuf, access: Access, persisted: bool) -> ApiResult<PathBuf> {
        let mut state = self.inner.lock().map_err(|_| poisoned())?;
        if let Some(existing) = state.grants.iter_mut().find(|grant| grant.path == path) {
            existing.access = existing.access.merge(access);
            existing.persisted |= persisted;
        } else {
            if state.grants.len() >= MAX_GRANTS {
                // Evicting silently would revoke an unrelated vault folder, so
                // this is an explicit error instead.
                return Err(ApiError::new(
                    "fs_scope_limit",
                    "Слишком много папок с доступом — закройте ненужные хранилища",
                ));
            }
            state.grants.push(Grant {
                path: path.clone(),
                access,
                persisted,
            });
        }
        let snapshot = StoredScope {
            grants: state
                .grants
                .iter()
                .filter(|grant| grant.persisted)
                .cloned()
                .collect(),
        };
        let store = state.store.clone();
        drop(state);
        persist(store, &snapshot);
        Ok(path)
    }

    /// Remembers a vault folder so it survives a restart. Called after the
    /// folder turned out to contain a vault.
    pub fn remember(&self, path: &Path) {
        if !looks_like_vault_folder(path) {
            return;
        }
        let Ok(canonical) = path.canonicalize() else {
            return;
        };
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if let Some(grant) = state
            .grants
            .iter_mut()
            .find(|grant| grant.path == canonical)
        {
            grant.persisted = true;
            let snapshot = StoredScope {
                grants: state
                    .grants
                    .iter()
                    .filter(|grant| grant.persisted)
                    .cloned()
                    .collect(),
            };
            let store = state.store.clone();
            drop(state);
            persist(store, &snapshot);
        }
    }

    /// Resolves a renderer-supplied path to a canonical path inside a grant.
    pub fn resolve(&self, raw: &str, needed: Access) -> ApiResult<PathBuf> {
        let candidate = normalize_candidate(raw)?;
        let state = self.inner.lock().map_err(|_| poisoned())?;
        let allowed = state.grants.iter().any(|grant| {
            let access_ok = if needed.allows_write() {
                grant.access.allows_write()
            } else {
                grant.access.allows_read()
            };
            access_ok && candidate.starts_with(&grant.path)
        });

        if allowed {
            Ok(candidate)
        } else {
            Err(scope_error())
        }
    }

    /// True when the path is a granted root or one of its ancestors: such a
    /// directory can never be the target of a recursive delete.
    pub fn is_root_or_ancestor(&self, path: &Path) -> bool {
        let Ok(state) = self.inner.lock() else {
            return true;
        };
        state
            .grants
            .iter()
            .any(|grant| grant.path == path || grant.path.starts_with(path))
    }

    #[cfg(test)]
    fn grants(&self) -> Vec<(PathBuf, Access, bool)> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .grants
                    .iter()
                    .map(|grant| (grant.path.clone(), grant.access, grant.persisted))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn persist(store: Option<PathBuf>, snapshot: &StoredScope) {
    if let Some(path) = store {
        if let Ok(json) = serde_json::to_vec_pretty(snapshot) {
            let _ = write_store(&path, &json);
        }
    }
}

fn dedup(grants: Vec<Grant>) -> Vec<Grant> {
    let mut out: Vec<Grant> = Vec::with_capacity(grants.len());
    for grant in grants {
        match out.iter_mut().find(|existing| existing.path == grant.path) {
            Some(existing) => {
                existing.access = existing.access.merge(grant.access);
                existing.persisted |= grant.persisted;
            }
            None => out.push(grant),
        }
    }
    out
}

fn write_store(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let atomic = atomicwrites::AtomicFile::new(path, atomicwrites::AllowOverwrite);
    atomic.write(|file| {
        use std::io::Write;
        file.write_all(bytes)?;
        file.sync_all()
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn scope_with(root: &Path) -> FsScope {
        let scope = FsScope::default();
        scope.grant(root, Access::ReadWrite).unwrap();
        scope
    }

    /// A folder that looks like a vault, so it can be remembered.
    fn vault_folder() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(VAULT_FILE), b"PMSV1...").unwrap();
        dir
    }

    #[test]
    fn resolves_paths_inside_a_granted_root() {
        let dir = tempdir().unwrap();
        let scope = scope_with(dir.path());
        let inside = dir.path().join("vault.bin");
        fs::write(&inside, b"x").unwrap();

        // `resolve` answers with the canonical path (on macOS `/var` is a symlink
        // to `/private/var`), so compare through `canonicalize`.
        assert_eq!(
            scope.resolve(&inside.to_string_lossy(), Access::Read).unwrap(),
            inside.canonicalize().unwrap()
        );
        let new_file = dir.path().join("attachments").join("a.pmsa");
        let resolved = scope
            .resolve(&new_file.to_string_lossy(), Access::Write)
            .unwrap();
        assert!(resolved.ends_with("attachments/a.pmsa"));
        assert_eq!(
            resolved.parent().unwrap(),
            dir.path().canonicalize().unwrap().join("attachments")
        );
    }

    #[test]
    fn rejects_paths_outside_grants() {
        let dir = tempdir().unwrap();
        let other = tempdir().unwrap();
        let scope = scope_with(dir.path());

        assert!(scope.resolve("/etc/passwd", Access::Read).is_err());
        assert!(scope
            .resolve(&other.path().join("x").to_string_lossy(), Access::Write)
            .is_err());
    }

    #[test]
    fn rejects_sibling_with_a_shared_prefix() {
        let root = tempdir().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let scope = scope_with(&vault);
        let sibling = root.path().join("vault-evil");
        fs::create_dir_all(&sibling).unwrap();

        assert!(scope
            .resolve(&sibling.join("x").to_string_lossy(), Access::Write)
            .is_err());
    }

    #[test]
    fn rejects_parent_traversal_and_relative_paths() {
        let dir = tempdir().unwrap();
        let scope = scope_with(dir.path());
        let escape = format!("{}/../escape", dir.path().display());

        assert!(scope.resolve(&escape, Access::Write).is_err());
        assert!(scope.resolve("relative/path", Access::Read).is_err());
        assert!(scope.resolve("", Access::Read).is_err());
    }

    #[test]
    fn rejects_symlinks_pointing_outside() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let secret = outside.path().join("secret");
        fs::write(&secret, b"secret").unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        let scope = scope_with(dir.path());
        #[cfg(unix)]
        assert!(scope.resolve(&link.to_string_lossy(), Access::Read).is_err());
    }

    #[test]
    fn read_only_grant_cannot_be_written() {
        let dir = tempdir().unwrap();
        let scope = FsScope::default();
        scope.grant(dir.path(), Access::Read).unwrap();
        let file = dir.path().join("keyfile");

        assert!(scope.resolve(&file.to_string_lossy(), Access::Read).is_ok());
        assert!(scope.resolve(&file.to_string_lossy(), Access::Write).is_err());
    }

    #[test]
    fn refuses_to_grant_the_filesystem_root_or_home() {
        let scope = FsScope::default();
        assert!(scope.grant(Path::new("/"), Access::ReadWrite).is_err());
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            assert!(scope.grant(&home, Access::ReadWrite).is_err());
        }
    }

    #[test]
    fn grants_are_merged_and_capped() {
        let scope = FsScope::default();
        let dir = tempdir().unwrap();
        scope.grant(dir.path(), Access::Read).unwrap();
        scope.grant(dir.path(), Access::Write).unwrap();
        let grants = scope.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].1, Access::ReadWrite);

        // A full grant table refuses new folders instead of silently evicting
        // an unrelated vault folder.
        let mut temporary = Vec::new();
        for _ in 0..(MAX_GRANTS - 1) {
            let extra = tempdir().unwrap();
            scope.grant(extra.path(), Access::Read).unwrap();
            temporary.push(extra);
        }
        let overflow = tempdir().unwrap();
        assert_eq!(
            scope.grant(overflow.path(), Access::Read).unwrap_err().code,
            "fs_scope_limit"
        );
        assert_eq!(scope.grants().len(), MAX_GRANTS);
    }

    #[test]
    fn only_vault_folders_are_remembered() {
        let dir = vault_folder();
        let config = tempdir().unwrap();
        let store = config.path().join("store.json");
        let cache = tempdir().unwrap();

        {
            let scope = FsScope::default();
            scope.attach(Some(store.clone()), vec![cache.path().to_path_buf()]);
            scope.grant(dir.path(), Access::ReadWrite).unwrap();
            assert!(!scope.grants().iter().any(|grant| grant.2), "not persisted yet");
            scope.remember(dir.path());
            assert!(scope.grants().iter().any(|grant| grant.2), "remembered");
        }

        let scope = FsScope::default();
        scope.attach(Some(store), vec![cache.path().to_path_buf()]);
        let file = dir.path().join(VAULT_FILE);
        assert!(scope.resolve(&file.to_string_lossy(), Access::Write).is_ok());
        let staged = cache.path().join("staging").join("pms.vault");
        assert!(scope.resolve(&staged.to_string_lossy(), Access::Write).is_ok());
    }

    #[test]
    fn forgets_folders_that_no_longer_hold_a_vault() {
        let dir = vault_folder();
        let config = tempdir().unwrap();
        let store = config.path().join("store.json");
        let cache = tempdir().unwrap();
        {
            let scope = FsScope::default();
            scope.attach(Some(store.clone()), vec![cache.path().to_path_buf()]);
            scope.grant(dir.path(), Access::ReadWrite).unwrap();
            scope.remember(dir.path());
        }
        fs::remove_file(dir.path().join(VAULT_FILE)).unwrap();

        let scope = FsScope::default();
        scope.attach(Some(store), vec![cache.path().to_path_buf()]);
        assert!(scope
            .resolve(&dir.path().join("other").to_string_lossy(), Access::Write)
            .is_err());
    }

    #[test]
    fn rejects_grant_store_content_that_grants_everything() {
        let root = tempdir().unwrap();
        let config = root.path().join("config");
        fs::create_dir_all(&config).unwrap();
        let store = config.join("fs-scope.json");
        let cache = root.path().join("cache");
        fs::create_dir_all(&cache).unwrap();

        // A renderer that can write the store could put "/" or its own home here.
        let payload = format!(
            r#"{{"grants":[{{"path":"/","access":"readWrite","persisted":true}},{{"path":"{}","access":"readOnly","persisted":true}}]}}"#,
            std::env::var("HOME").unwrap_or_default()
        );
        fs::write(&store, payload).unwrap();

        let scope = FsScope::default();
        scope.attach(Some(store), vec![cache.clone()]);

        assert!(scope.resolve("/etc/passwd", Access::Read).is_err());
        let staged = cache.join("staging");
        assert!(scope.resolve(&staged.to_string_lossy(), Access::Write).is_ok());
    }

    #[test]
    fn grants_a_save_target_that_does_not_exist_yet() {
        let dir = tempdir().unwrap();
        let scope = FsScope::default();

        let target = dir.path().join("export.pmsvault");
        assert!(!target.exists());
        let granted = scope
            .grant_target(&target, Access::Write)
            .expect("a save target must be grantable");
        assert_eq!(
            scope.resolve(&granted.to_string_lossy(), Access::Write).unwrap(),
            granted
        );

        // The target itself is the only thing that becomes writable.
        assert!(scope
            .resolve(&dir.path().join("other.bin").to_string_lossy(), Access::Write)
            .is_err());
        // Traversal in the name is refused.
        assert!(scope
            .grant_target(&dir.path().join("..").join("x"), Access::Write)
            .is_err());
    }

    #[test]
    fn save_target_grants_are_not_remembered() {
        let dir = tempdir().unwrap();
        let scope = FsScope::default();
        scope.grant(dir.path(), Access::ReadWrite).unwrap();
        scope
            .grant_target(&dir.path().join("export.pmsvault"), Access::Write)
            .unwrap();
        assert!(scope.grants().iter().all(|grant| !grant.2));
    }

    #[test]
    fn granted_root_cannot_be_deleted() {
        let dir = vault_folder();
        let canonical = dir.path().canonicalize().unwrap();
        let scope = scope_with(dir.path());
        assert!(scope.is_root_or_ancestor(&canonical));
        assert!(scope.is_root_or_ancestor(Path::new("/")));
        assert!(!scope.is_root_or_ancestor(&canonical.join("staging")));
    }
}
