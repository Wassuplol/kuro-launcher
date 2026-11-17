mod error;
mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

pub use error::{FailureCode, LaunchError};
use settings::Settings;

pub struct AppState {
    pub data_root: PathBuf,
    pub settings: Mutex<Settings>,
    pub settings_error: Option<LaunchError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub data_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_error: Option<LaunchError>,
}

fn poisoned() -> LaunchError {
    LaunchError::new(
        FailureCode::Internal,
        "The settings lock was poisoned by an earlier fault.",
    )
}

#[tauri::command]
fn app_info(app: tauri::AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        identifier: app.config().identifier.clone(),
        data_root: state.data_root.to_string_lossy().to_string(),
        settings_error: state.settings_error.clone(),
    }
}

#[tauri::command]
fn settings_load(state: State<'_, AppState>) -> Result<Settings, LaunchError> {
    let guard = state.settings.lock().map_err(|_| poisoned())?;
    Ok(guard.clone())
}

#[tauri::command]
fn settings_save(state: State<'_, AppState>, next: Settings) -> Result<Settings, LaunchError> {
    let next = settings::normalise(next);
    settings::save(&state.data_root, &next)?;
    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    *guard = next.clone();
    Ok(next)
}

/// Recovery for a settings file that cannot be read. Writes the defaults back
/// and returns them, so a broken file never leaves the app without a way out.
#[tauri::command]
fn settings_reset(state: State<'_, AppState>) -> Result<Settings, LaunchError> {
    let fresh = Settings::default();
    settings::save(&state.data_root, &fresh)?;
    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    *guard = fresh.clone();
    Ok(fresh)
}

/// Opens the data root in Explorer. Spawned directly so the frontend never
/// receives a filesystem path it could pass somewhere else.
#[tauri::command]
fn open_data_root(state: State<'_, AppState>) -> Result<(), LaunchError> {
    std::process::Command::new("explorer")
        .arg(&state.data_root)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Explorer could not open the data folder: {e}"),
            )
        })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let root = settings::data_root()?;
            std::fs::create_dir_all(&root)?;

            // A settings file that cannot be read must not stop the app from
            // starting. The defaults take over and the UI is told why, so the
            // person still has a way out.
            let (loaded, settings_error) = match settings::load(&root) {
                Ok(settings) => (settings, None),
                Err(error) => {
                    eprintln!("settings could not be read: {error}");
                    (Settings::default(), Some(error))
                }
            };

            app.manage(AppState {
                data_root: root,
                settings: Mutex::new(loaded),
                settings_error,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_load,
            settings_save,
            settings_reset,
            open_data_root
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window failed to start");
}
