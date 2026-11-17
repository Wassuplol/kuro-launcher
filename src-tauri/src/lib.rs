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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub data_root: String,
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
            let loaded = settings::load(&root)?;
            app.manage(AppState {
                data_root: root,
                settings: Mutex::new(loaded),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_load,
            settings_save,
            open_data_root
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window failed to start");
}
