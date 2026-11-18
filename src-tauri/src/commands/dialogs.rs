//! Native file/folder pickers that double as the only way to grant filesystem
//! access. The dialog has to live in Rust: if the renderer could tell us which
//! path it "picked", a compromised webview could grant itself `/` and defeat
//! the scope in [`super::fs_scope`].

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::fs_scope::Access;
use crate::commands::{ApiError, ApiResult, AppState};

const DIALOG_TIMEOUT: Duration = Duration::from_secs(180);

type Picked = Box<dyn FnOnce(Option<tauri_plugin_dialog::FilePath>) + Send>;

fn into_path(file: tauri_plugin_dialog::FilePath) -> Option<PathBuf> {
    file.into_path().ok()
}

/// Shows a dialog on a worker thread: the plugin dispatches the window to the
/// main thread and calls the callback back, so blocking one worker is fine.
fn run<F>(build: F) -> ApiResult<Option<PathBuf>>
where
    F: FnOnce(Picked) + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    tauri::async_runtime::spawn_blocking(move || {
        build(Box::new(move |picked| {
            let _ = sender.send(picked);
        }));
    });

    let picked = receiver
        .recv_timeout(DIALOG_TIMEOUT)
        .map_err(|_| ApiError::new("dialog", "Диалог выбора файла был прерван"))?;
    Ok(picked.and_then(into_path))
}

fn label(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Folder picking is desktop-only: the plugin has no folder dialog on Android,
/// where the app picks a SAF tree through `tauri-plugin-android-fs` instead.
#[cfg(desktop)]
#[tauri::command]
pub async fn dialog_pick_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> ApiResult<Option<String>> {
    let text = label(title, "Выберите папку");
    let picked = run(move |callback| {
        app.dialog().file().set_title(&text).pick_folder(callback);
    })?;

    grant_and_describe(&state, picked, Access::ReadWrite)
}

#[cfg(mobile)]
#[tauri::command]
pub async fn dialog_pick_folder(
    _app: AppHandle,
    _state: State<'_, AppState>,
    _title: Option<String>,
) -> ApiResult<Option<String>> {
    Err(ApiError::new(
        "dialog_unsupported",
        "На этом устройстве папку выбирают системным файловым менеджером",
    ))
}

#[tauri::command]
pub async fn dialog_pick_file(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> ApiResult<Option<String>> {
    let text = label(title, "Выберите файл");
    let picked = run(move |callback| {
        app.dialog().file().set_title(&text).pick_file(callback);
    })?;

    // A file the user opened is readable, nothing more: the app has no reason
    // to rewrite a keyfile or an import archive.
    grant_and_describe(&state, picked, Access::Read)
}

#[tauri::command]
pub async fn dialog_pick_save_file(
    app: AppHandle,
    state: State<'_, AppState>,
    suggested_name: Option<String>,
    title: Option<String>,
) -> ApiResult<Option<String>> {
    let text = label(title, "Сохранить файл");
    let suggested = suggested_name.unwrap_or_default();
    let picked = run(move |callback| {
        let mut dialog = app.dialog().file().set_title(&text);
        if !suggested.is_empty() {
            dialog = dialog.set_file_name(&suggested);
        }
        dialog.save_file(callback);
    })?;

    // A save dialog returns a path that does not exist yet, so it cannot be
    // canonicalized like a picked folder.
    match picked {
        Some(path) => {
            let granted = state.fs_scope.grant_target(&path, Access::Write)?;
            Ok(Some(granted.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

fn grant_and_describe(
    state: &State<'_, AppState>,
    picked: Option<PathBuf>,
    access: Access,
) -> ApiResult<Option<String>> {
    match picked {
        Some(path) => {
            let granted = state.fs_scope.grant(&path, access)?;
            Ok(Some(granted.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}
