//! Временный файл, который убирается сам: оборванное скачивание не оставляет
//! мусора и не подменяет настоящий сейф обрезанным.

use std::path::{Path, PathBuf};

pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_error) => {
                // Молча: путь уже не существует или каталог исчез. Ошибка здесь
                // ничего не даёт вызывающему коду и не должна его ломать.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pms.vault.part");
        let guard = TempFile::at(path.clone());
        std::fs::write(guard.path(), b"partial").unwrap();
        assert!(path.exists());
        drop(guard);
        assert!(!path.exists());
    }
}
