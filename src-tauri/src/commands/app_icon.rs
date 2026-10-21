#[cfg(not(target_os = "android"))]
use std::sync::mpsc;

use serde::Deserialize;
use tauri::AppHandle;

#[cfg(not(target_os = "android"))]
use crate::commands::ApiError;
use crate::commands::ApiResult;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppIcon {
    BlueShield,
    GreenLock,
    GoldKey,
}

impl AppIcon {
    #[cfg(target_os = "android")]
    fn as_str(self) -> &'static str {
        match self {
            Self::BlueShield => "blue-shield",
            Self::GreenLock => "green-lock",
            Self::GoldKey => "gold-key",
        }
    }

    #[cfg(not(target_os = "android"))]
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::BlueShield => include_bytes!("../../assets/appicons/01-blue-shield.png"),
            Self::GreenLock => include_bytes!("../../assets/appicons/02-green-lock.png"),
            Self::GoldKey => include_bytes!("../../assets/appicons/03-gold-key.png"),
        }
    }
}

#[tauri::command]
pub fn app_icon_set(icon: AppIcon, app: AppHandle) -> ApiResult<()> {
    #[cfg(target_os = "android")]
    {
        android_icon::set(&app, icon)?;
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let bytes = icon.bytes();
        let (sender, receiver) = mpsc::channel();
        app.run_on_main_thread(move || {
            let result = imp::set(bytes).map_err(|error| error.to_string());
            let _ = sender.send(result);
        })
        .map_err(|error| ApiError::new("app_icon", error.to_string()))?;
        return receiver
            .recv()
            .map_err(|error| ApiError::new("app_icon", error.to_string()))?
            .map_err(|error| ApiError::new("app_icon", error));
    }

    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(target_os = "android")]
pub fn android_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    use tauri::Manager;
    tauri::plugin::Builder::new("pms-icon")
        .setup(|app, api| {
            let handle = api.register_android_plugin("dev.pms.vault", "IconPlugin")?;
            app.manage(android_icon::IconHandle(handle));
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
mod android_icon {
    use serde::Serialize;
    use tauri::plugin::PluginHandle;
    use tauri::{AppHandle, Manager, Runtime};

    use super::AppIcon;
    use crate::commands::{ApiError, ApiResult};

    pub struct IconHandle<R: Runtime>(pub PluginHandle<R>);

    #[derive(Serialize)]
    struct IconPayload {
        icon: &'static str,
    }

    pub fn set<R: Runtime>(app: &AppHandle<R>, icon: AppIcon) -> ApiResult<()> {
        app.state::<IconHandle<R>>()
            .0
            .run_mobile_plugin("setIcon", IconPayload { icon: icon.as_str() })
            .map_err(|error| ApiError::new("app_icon", error.to_string()))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    pub fn set(bytes: &'static [u8]) -> Result<(), String> {
        let marker = MainThreadMarker::new().ok_or("app icon requires the main thread")?;
        let data = NSData::with_bytes(bytes);
        let image: Retained<NSImage> = NSImage::initWithData(NSImage::alloc(), &data)
            .ok_or("could not decode app icon")?;
        let application = NSApplication::sharedApplication(marker);
        unsafe { application.setApplicationIconImage(Some(&image)) };
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "android")))]
mod imp {
    pub fn set(_bytes: &'static [u8]) -> Result<(), String> {
        Ok(())
    }
}
