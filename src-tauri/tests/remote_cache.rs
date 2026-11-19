//! Каталог кэша серверного хранилища и границы его доступа.

use pms_lib::testing::{fs_scope_probe, remote_cache};

#[test]
fn cache_dir_has_layout_and_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    pms_lib::testing::set_data_dir(dir.path().to_path_buf());

    let path = remote_cache::cache_dir("profile-1").unwrap();
    assert!(path.ends_with("remote/profile-1"));
    assert!(path.join("attachments").is_dir());
    assert!(path.join("backups").is_dir());
    assert_eq!(remote_cache::cache_dir("profile-1").unwrap(), path);
}

#[test]
fn profile_id_cannot_escape_the_cache_root() {
    let dir = tempfile::tempdir().unwrap();
    pms_lib::testing::set_data_dir(dir.path().to_path_buf());

    for bad in ["..", "../etc", "a/b", "a\\b", "", "профиль", "."] {
        let error = remote_cache::cache_dir(bad).unwrap_err();
        assert_eq!(error.code, "bad_profile_id", "приняли {bad:?}: {error:?}");
    }
}

#[test]
fn only_the_cache_subtree_is_granted() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    std::fs::create_dir_all(app_data.join("remote/profile-1")).unwrap();
    std::fs::write(app_data.join("fs-scope.json"), b"{}").unwrap();

    let scope = fs_scope_probe::scope_with_always(&[app_data.join("remote")]);

    assert!(scope
        .resolve(
            &app_data
                .join("remote/profile-1/pms.vault")
                .to_string_lossy(),
            true
        )
        .is_ok());
    assert!(
        scope
            .resolve(&app_data.join("fs-scope.json").to_string_lossy(), false)
            .is_err(),
        "хранилище грантов не должно быть доступно"
    );
}
