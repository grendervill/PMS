use std::sync::mpsc;

use serde::Deserialize;
use tauri::AppHandle;

use crate::commands::{ApiError, ApiResult};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppIcon {
    BlueShield,
    GreenLock,
    GoldKey,
}

impl AppIcon {
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
    let bytes = icon.bytes();
    let (sender, receiver) = mpsc::channel();
    app.run_on_main_thread(move || {
        let result = imp::set(bytes).map_err(|error| error.to_string());
        let _ = sender.send(result);
    })
    .map_err(|error| ApiError::new("app_icon", error.to_string()))?;
    receiver
        .recv()
        .map_err(|error| ApiError::new("app_icon", error.to_string()))?
        .map_err(|error| ApiError::new("app_icon", error))
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

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn set(_bytes: &'static [u8]) -> Result<(), String> {
        Ok(())
    }
}
