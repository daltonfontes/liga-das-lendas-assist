pub mod assistant;
pub mod config;
pub mod features;
pub mod lcu;
pub mod live;
pub mod state;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tauri::{Emitter, Manager};

use config::AssistantConfig;
use state::{AssistantState, Store};

struct AppState {
    store: Arc<Store>,
    config: Arc<RwLock<AssistantConfig>>,
    config_path: PathBuf,
}

#[tauri::command]
fn get_state(app: tauri::State<AppState>) -> AssistantState {
    app.store.get()
}

#[tauri::command]
fn get_config(app: tauri::State<AppState>) -> AssistantConfig {
    app.config.read().unwrap().clone()
}

#[tauri::command]
fn set_config(app: tauri::State<AppState>, config: AssistantConfig) -> Result<(), String> {
    config::save(&app.config_path, &config).map_err(|e| e.to_string())?;
    *app.config.write().unwrap() = config;
    Ok(())
}

#[tauri::command]
fn set_automation_paused(app: tauri::State<AppState>, paused: bool) {
    app.store.set_automation_paused(paused);
}

#[tauri::command]
fn get_champions(app: tauri::State<AppState>) -> Vec<String> {
    app.store.icons.names()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let config = Arc::new(RwLock::new(config::load(&config_path)));
            let handle = app.handle().clone();
            let store = Arc::new(Store::new(move |state| {
                let _ = handle.emit("state", state);
            }));
            app.manage(AppState {
                store: store.clone(),
                config: config.clone(),
                config_path,
            });
            tauri::async_runtime::spawn(assistant::run(store, config));
            Ok(())
        })
        // Ícones dos campeões: champ://localhost/<nome ou id> (http://champ.localhost no Windows).
        .register_asynchronous_uri_scheme_protocol("champ", |ctx, request, responder| {
            let store = ctx.app_handle().state::<AppState>().store.clone();
            let key =
                percent_encoding::percent_decode_str(request.uri().path().trim_start_matches('/'))
                    .decode_utf8_lossy()
                    .into_owned();
            tauri::async_runtime::spawn(async move {
                let response = match store.icons.icon(&key).await {
                    Some(png) => tauri::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=86400")
                        .body(png.to_vec()),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new()),
                };
                responder.respond(response.expect("resposta HTTP válida"));
            });
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_config,
            set_config,
            set_automation_paused,
            get_champions
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Liga das Lendas Assist");
}
