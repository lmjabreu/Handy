pub mod audio;
pub mod history;
pub mod models;
pub mod transcription;

use crate::settings::{
    get_settings, update_checks_forced_disabled, write_settings, AppSettings, LogLevel,
};
use crate::utils::cancel_current_operation;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
#[specta::specta]
pub fn cancel_operation(app: AppHandle) {
    cancel_current_operation(&app);
}

/// Restarts Handy, after an update or for the onboarding Accessibility step.
///
/// On macOS it goes through LaunchServices (`open`) once this process has
/// exited, because Tauri's restart runs the binary directly and ActivityKit
/// refuses an app started that way, so the Dynamic Island stays off until the
/// next manual launch. Elsewhere, and outside an .app bundle, it is Tauri's
/// restart.
#[tauri::command]
#[specta::specta]
pub fn relaunch_app(app: AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = std::env::current_exe()
        .ok()
        .and_then(|exe| app_bundle_of(&exe))
    {
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let spawned = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(RELAUNCH_SCRIPT)
            .arg("handy-relaunch")
            .arg(std::process::id().to_string())
            .arg(&bundle)
            .args(&args)
            .spawn();
        match spawned {
            Ok(_) => {
                app.exit(0);
                return;
            }
            Err(e) => {
                log::warn!("Relaunch through LaunchServices failed, restarting directly: {e}")
            }
        }
    }
    app.restart();
}

/// Waits for the old process ($1) to exit, so the single-instance check does
/// not hand the new one back to it, then opens the bundle ($2) with the
/// original arguments.
#[cfg(target_os = "macos")]
const RELAUNCH_SCRIPT: &str = r#"pid=$1; bundle=$2; shift 2
while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
if [ "$#" -gt 0 ]; then exec /usr/bin/open "$bundle" --args "$@"; else exec /usr/bin/open "$bundle"; fi"#;

/// The .app bundle an executable sits in (`X.app/Contents/MacOS/exe`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn app_bundle_of(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    is_bundle.then(|| bundle.to_path_buf())
}

#[cfg(test)]
mod relaunch_tests {
    use super::app_bundle_of;
    use std::path::{Path, PathBuf};

    #[test]
    fn finds_the_bundle_of_an_app_executable() {
        assert_eq!(
            app_bundle_of(Path::new("/Applications/Handy.app/Contents/MacOS/handy")),
            Some(PathBuf::from("/Applications/Handy.app"))
        );
    }

    #[test]
    fn a_bare_binary_has_no_bundle() {
        assert_eq!(app_bundle_of(Path::new("/usr/local/bin/handy")), None);
        assert_eq!(
            app_bundle_of(Path::new("/repo/src-tauri/target/debug/handy")),
            None
        );
    }
}

/// Whether this Mac can show Handy in the Dynamic Island, so the settings only
/// offer the choice where it applies.
#[tauri::command]
#[specta::specta]
pub fn is_dynamic_island_available() -> bool {
    crate::island::available()
}

#[tauri::command]
#[specta::specta]
pub fn is_portable() -> bool {
    crate::portable::is_portable()
}

#[tauri::command]
#[specta::specta]
pub fn is_update_checks_locked() -> bool {
    update_checks_forced_disabled()
}

#[tauri::command]
#[specta::specta]
pub fn get_app_dir_path(app: AppHandle) -> Result<String, String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(app_data_dir.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(get_settings(&app))
}

#[tauri::command]
#[specta::specta]
pub fn get_default_settings() -> Result<AppSettings, String> {
    Ok(crate::settings::get_default_settings())
}

#[tauri::command]
#[specta::specta]
pub fn get_log_dir_path(app: AppHandle) -> Result<String, String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    Ok(log_dir.to_string_lossy().to_string())
}

#[specta::specta]
#[tauri::command]
pub fn set_log_level(app: AppHandle, level: LogLevel) -> Result<(), String> {
    let tauri_log_level: tauri_plugin_log::LogLevel = level.into();
    let log_level: log::Level = tauri_log_level.into();
    // Update the file log level atomic so the filter picks up the new level
    crate::FILE_LOG_LEVEL.store(
        log_level.to_level_filter() as u8,
        std::sync::atomic::Ordering::Relaxed,
    );

    let mut settings = get_settings(&app);
    settings.log_level = level;
    write_settings(&app, settings);

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_recordings_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let recordings_dir = app_data_dir.join("recordings");

    let path = recordings_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open recordings folder: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    let path = log_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open log directory: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_app_data_dir(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let path = app_data_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open app data directory: {}", e))?;

    Ok(())
}

/// Check if Apple Intelligence is available on this device.
/// Called by the frontend when the user selects Apple Intelligence provider.
#[specta::specta]
#[tauri::command]
pub fn check_apple_intelligence_available() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        crate::apple_intelligence::check_apple_intelligence_availability()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        false
    }
}

/// Try to initialize Enigo (keyboard/mouse simulation).
/// On macOS, this will return an error if accessibility permissions are not granted.
#[specta::specta]
#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    use crate::input::EnigoState;

    // Check if already initialized
    if app.try_state::<EnigoState>().is_some() {
        log::debug!("Enigo already initialized");
        return Ok(());
    }

    // Try to initialize
    match EnigoState::new() {
        Ok(enigo_state) => {
            app.manage(enigo_state);
            log::info!("Enigo initialized successfully after permission grant");
            Ok(())
        }
        Err(e) => {
            if cfg!(target_os = "macos") {
                log::warn!(
                    "Failed to initialize Enigo: {} (accessibility permissions may not be granted)",
                    e
                );
            } else {
                log::warn!("Failed to initialize Enigo: {}", e);
            }
            Err(format!("Failed to initialize input system: {}", e))
        }
    }
}

/// Marker state to track if shortcuts have been initialized.
pub struct ShortcutsInitialized;

/// Initialize keyboard shortcuts.
/// On macOS, this should be called after accessibility permissions are granted.
/// This is idempotent - calling it multiple times is safe.
#[specta::specta]
#[tauri::command]
pub fn initialize_shortcuts(app: AppHandle) -> Result<(), String> {
    // Check if already initialized
    if app.try_state::<ShortcutsInitialized>().is_some() {
        log::debug!("Shortcuts already initialized");
        return Ok(());
    }

    // Initialize shortcuts
    crate::shortcut::init_shortcuts(&app);

    // Mark as initialized before reconciling the macOS Secure Input fallback.
    app.manage(ShortcutsInitialized);
    crate::secure_input::reconcile_fallback(&app);

    log::info!("Shortcuts initialized successfully");
    Ok(())
}

/// Mark onboarding as finished; called when its last screen closes, so quitting
/// midway resumes the flow.
#[specta::specta]
#[tauri::command]
pub fn complete_onboarding(app: AppHandle) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.onboarding_completed = true;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// A macOS privacy permission Handy asks for during onboarding.
#[derive(Clone, Copy, Debug, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum MacPermission {
    Accessibility,
    Microphone,
}

/// Clears Handy's macOS grant for `permission` before asking for it. A grant is
/// tied to the app's code signature, so after an update the old entry stays in
/// System Settings without matching the new build: macOS neither honors it nor
/// asks again, and onboarding waits forever (#1618). Only called while the
/// permission reads as not granted, so there is no working grant to lose.
#[tauri::command]
#[specta::specta]
pub fn reset_macos_permission(app: AppHandle, permission: MacPermission) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let service = match permission {
            MacPermission::Accessibility => "Accessibility",
            MacPermission::Microphone => "Microphone",
        };
        let identifier = app.config().identifier.clone();
        let output = std::process::Command::new("/usr/bin/tccutil")
            .args(["reset", service, &identifier])
            .output()
            .map_err(|e| format!("Failed to run tccutil: {e}"))?;
        if !output.status.success() {
            // Asking is still worth trying: on a first install there may be
            // nothing to reset.
            log::warn!(
                "tccutil reset {service} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, permission);
    Ok(())
}
