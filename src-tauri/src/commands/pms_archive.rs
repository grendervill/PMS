use tauri::{AppHandle, Manager};

use crate::commands::fs_scope::Access;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::pms_archive;

#[tauri::command]
pub async fn pms_export(app: AppHandle, folder: String, destination: String) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Read)?;
        let destination = state.fs_scope.resolve(&destination, Access::Write)?;
        pms_archive::export(&folder, &destination).map_err(ApiError::from)
    })
    .await
    .map_err(|error| ApiError::new("pms_export", error.to_string()))?
}

#[tauri::command]
pub async fn pms_import(app: AppHandle, archive: String, destination: String) -> ApiResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let archive = state.fs_scope.resolve(&archive, Access::Read)?;
        let destination = state.fs_scope.resolve(&destination, Access::Write)?;
        pms_archive::import(&archive, &destination).map_err(ApiError::from)?;
        Ok(destination.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| ApiError::new("pms_import", error.to_string()))?
}
