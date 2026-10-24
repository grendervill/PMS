use tauri::AppHandle;

/// Applies Android screenshot protection. This is best-effort: the setting is
/// stored in the encrypted vault regardless, and the flag is applied again on
/// the next resume.
pub fn apply(app: &AppHandle, enabled: bool) {
    #[cfg(target_os = "android")]
    {
        let app = app.clone();
        if let Err(error) = app.run_on_main_thread(move || {
            if let Err(error) = imp::set(enabled) {
                eprintln!("PMS secure screen: {error}");
            }
        }) {
            eprintln!("PMS secure screen: failed to reach the main thread: {error}");
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = (app, enabled);
}

#[cfg(target_os = "android")]
mod imp {
    use jni::JNIEnv;
    use jni::errors::Error as JniError;
    use jni::objects::{JClass, JObject, JValue};
    use jni::JavaVM;

    fn failed(env: &mut JNIEnv, context: &str, error: JniError) -> String {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        format!("{context}: {error}")
    }

    /// Resolves the running activity through the application class loader and
    /// calls `MainActivity.setSecureScreen`, which stores the preference and
    /// toggles `WindowManager.LayoutParams.FLAG_SECURE`.
    pub fn set(enabled: bool) -> Result<(), String> {
        let context = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
            .map_err(|error| format!("JavaVM: {error}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|error| format!("attach: {error}"))?;
        let application = unsafe { JObject::from_raw(context.context().cast()) };

        let loader = env
            .call_method(
                &application,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .map_err(|error| failed(&mut env, "getClassLoader", error))?
            .l()
            .map_err(|error| failed(&mut env, "getClassLoader result", error))?;
        let class_name = env
            .new_string("dev.pms.vault.MainActivity")
            .map_err(|error| failed(&mut env, "class name", error))?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&class_name)],
            )
            .map_err(|error| failed(&mut env, "loadClass", error))?
            .l()
            .map_err(|error| failed(&mut env, "loadClass result", error))?;
        let class = JClass::from(class);
        let activity = env
            .call_static_method(
                &class,
                "current",
                "()Ldev/pms/vault/MainActivity;",
                &[],
            )
            .map_err(|error| failed(&mut env, "current", error))?
            .l()
            .map_err(|error| failed(&mut env, "current result", error))?;
        if activity.is_null() {
            return Ok(());
        }

        env.call_method(
            &activity,
            "setSecureScreen",
            "(Z)V",
            &[JValue::Bool(u8::from(enabled))],
        )
        .map_err(|error| failed(&mut env, "setSecureScreen", error))?;
        Ok(())
    }
}
