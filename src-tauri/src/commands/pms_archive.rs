use crate::commands::{ApiError, ApiResult};
use crate::core::pms_archive;

#[tauri::command]
pub fn pms_export(folder: String, destination: String) -> ApiResult<()> {
    pms_archive::export(std::path::Path::new(&folder), std::path::Path::new(&destination))
        .map_err(ApiError::from)
}

#[tauri::command]
pub fn pms_import(archive: String, destination: String) -> ApiResult<String> {
    let target = std::path::Path::new(&destination);
    pms_archive::import(std::path::Path::new(&archive), target).map_err(ApiError::from)?;
    Ok(target.to_string_lossy().into_owned())
}
