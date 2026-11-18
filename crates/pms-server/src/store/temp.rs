//! Временные файлы, которые убираются сами.
//!
//! Загрузка сейфа или блоба пишет во временный файл и переименовывает его при
//! успехе. Любая отмена (обрыв, таймаут, выключение сервера) должна убирать
//! временный файл: без этого диск заполняется мусором, которого не видит ни
//! один счётчик квоты.

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
            // Файл уже переименован или не создавался — это нормально.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(path = %self.path.display(), %error, "не удалось убрать временный файл");
            }
        }
    }
}
