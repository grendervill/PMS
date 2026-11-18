//! Copying a secret to the clipboard, marked as sensitive where the platform
//! supports it.
//!
//! Clearing the clipboard after a timeout (see `src/lib/clipboard.ts`) only helps
//! while the value is there: in between, macOS Universal Clipboard, Windows
//! clipboard history and third-party clipboard managers happily keep a copy. Both
//! platforms have a way to say "do not record this".

use tauri::AppHandle;

use crate::commands::ApiResult;

/// Copies `value` and asks the system not to persist or sync it.
#[tauri::command]
pub fn clipboard_copy_secret(value: String, app: AppHandle) -> ApiResult<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = &app;
        imp::copy_marked(&value);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows needs the `windows` crate for its clipboard formats and
        // Android has no such marking, so this falls back to a plain copy.
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard()
            .write_text(value)
            .map_err(|error| crate::commands::ApiError::new("clipboard", error.to_string()))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::{NSArray, NSString};

    /// De-facto standard markers from the Mac clipboard-manager community: a
    /// "concealed" item is skipped by history managers and by Universal
    /// Clipboard, a "transient" one is not persisted at all.
    const CONCEALED: &str = "org.nspasteboard.ConcealedType";
    const TRANSIENT: &str = "org.nspasteboard.TransientType";

    pub fn copy_marked(value: &str) {
        let pasteboard = NSPasteboard::generalPasteboard();

        // The declared types must include the plain one, otherwise the paste
        // action itself cannot read the value.
        let plain: objc2::rc::Retained<NSString> = unsafe { NSPasteboardTypeString }.into();
        let types = vec![
            plain,
            NSString::from_str(CONCEALED),
            NSString::from_str(TRANSIENT),
        ];
        let array = NSArray::from_retained_slice(&types);

        pasteboard.clearContents();
        // `owner` is unused for a pasteboard, but the selector requires a pointer.
        unsafe { pasteboard.declareTypes_owner(&array, None) };
        pasteboard.setString_forType(&NSString::from_str(value), unsafe { NSPasteboardTypeString });
    }
}
