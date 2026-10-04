use serde_json::Value;
#[cfg(target_os = "android")]
use tauri::Manager;
#[cfg(target_os = "android")]
struct Native(tauri::plugin::PluginHandle<tauri::Wry>);
#[tauri::command]
pub async fn private_connection(
    app: tauri::AppHandle,
    action: String,
    url: Option<String>,
    code: Option<String>,
) -> Result<Value, String> {
    if !["status", "pair", "check", "forget"].contains(&action.as_str()) {
        return Err("Invalid action".into());
    }
    #[cfg(target_os = "android")]
    return tauri::async_runtime::spawn_blocking(move || {
        app.state::<Native>()
            .0
            .run_mobile_plugin(
                "connection",
                serde_json::json!({"action":action,"url":url,"code":code}),
            )
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Connection worker failed".to_string())?;
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, url, code);
        Err("Private connection requires Android".into())
    }
}
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("private-connection")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(Native(api.register_android_plugin(
                "com.tmw.companion",
                "ConnectionPlugin",
            )?));
            #[cfg(not(target_os = "android"))]
            let _ = (app, api);
            Ok(())
        })
        .build()
}

#[tauri::command]
pub async fn mobile_storage(app: tauri::AppHandle, mut args: Value) -> Result<Value, String> {
    if let Some(query) = args["query"].as_str() {
        let query = query.to_string();
        args["query"] = serde_json::json!(tmw_japanese_core::dictionary::normalize_query(&query));
        args["romajiQuery"] =
            serde_json::json!(tmw_japanese_core::readings::normalize_romaji(&query));
    }
    #[cfg(target_os = "android")]
    return tauri::async_runtime::spawn_blocking(move || {
        app.state::<Native>()
            .0
            .run_mobile_plugin("mobile", args)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, args);
        Err("Mobile storage requires Android".into())
    }
}
