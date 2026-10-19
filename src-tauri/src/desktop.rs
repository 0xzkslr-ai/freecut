use crate::{
    imaging::{self, Image, Point, Preview},
    platform::{self, Monitor},
    settings::{self, Mode, Settings},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Serialize)]
struct SessionInfo {
    id: u64,
    width: u32,
    height: u32,
    mode: Mode,
}
struct Capture {
    info: SessionInfo,
    image: Image,
    png: Arc<Vec<u8>>,
    background: bool,
}
struct ResultImage {
    info: Preview,
    image: Image,
    png: Arc<Vec<u8>>,
}
struct Data {
    settings: Settings,
    settings_path: PathBuf,
    active_hotkey: Option<String>,
    hotkey_error: Option<String>,
    tray_available: bool,
    capturing: bool,
    modal: bool,
    next_id: u64,
    capture: Option<Capture>,
    result: Option<ResultImage>,
}
struct AppState(Mutex<Data>);
impl AppState {
    fn lock(&self) -> MutexGuard<'_, Data> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitialState {
    settings: Settings,
    monitors: Vec<Monitor>,
    preview: Option<Preview>,
    hotkey_error: Option<String>,
    tray_available: bool,
}

fn window(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or("应用窗口不可用，请重新启动。".into())
}
fn require(window: &WebviewWindow, label: &str) -> Result<(), String> {
    if window.label() == label {
        Ok(())
    } else {
        Err("当前窗口无法执行此操作。".into())
    }
}
fn show_main(app: &AppHandle) {
    if app.state::<AppState>().lock().capturing {
        return;
    }
    if let Ok(main) = window(app, "main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}
fn report_error(app: &AppHandle, error: String) {
    show_main(app);
    let _ = app.emit_to("main", "capture-error", error);
}

#[tauri::command]
fn initial_state(app: AppHandle, window: WebviewWindow) -> Result<InitialState, String> {
    require(&window, "main")?;
    let monitors = platform::monitors().unwrap_or_default();
    let state = app.state::<AppState>();
    let data = state.lock();
    Ok(InitialState {
        settings: data.settings.clone(),
        monitors,
        preview: data.result.as_ref().map(|r| r.info.clone()),
        hotkey_error: data.hotkey_error.clone(),
        tray_available: data.tray_available,
    })
}

#[tauri::command]
fn frontend_ready(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "main")?;
    if !std::env::args().any(|arg| arg == "--background")
        || !app.state::<AppState>().lock().tray_available
    {
        show_main(&app);
    }
    Ok(())
}

#[tauri::command]
fn list_monitors() -> Result<Vec<Monitor>, String> {
    platform::monitors()
}

fn begin_capture(app: &AppHandle, monitor_id: Option<String>) -> Result<(), String> {
    let main = window(app, "main")?;
    let overlay = window(app, "overlay")?;
    let state = app.state::<AppState>();
    let (mode, background) = {
        let mut data = state.lock();
        if data.capturing || data.modal {
            return Ok(());
        }
        let background = data.tray_available
            && (!main.is_visible().unwrap_or(true) || main.is_minimized().unwrap_or(false));
        data.capturing = true;
        (data.settings.mode, background)
    };
    let result = (|| {
        let monitors = platform::monitors()?;
        let monitor = match monitor_id {
            Some(id) => monitors
                .into_iter()
                .find(|m| m.id == id)
                .ok_or("所选显示器已断开，请重新选择。")?,
            None => platform::cursor_monitor(&monitors)?,
        };
        overlay.hide().map_err(|_| "无法隐藏选区窗口。")?;
        // Tauri uses per-monitor DPI awareness. These operations use physical desktop coordinates.
        overlay
            .set_position(PhysicalPosition::new(monitor.x, monitor.y))
            .map_err(|_| "无法定位选区窗口。")?;
        overlay
            .set_size(PhysicalSize::new(monitor.width, monitor.height))
            .map_err(|_| "无法设置选区窗口尺寸。")?;
        let hwnd = main.hwnd().map_err(|_| "无法访问主窗口。")?;
        main.hide().map_err(|_| "无法隐藏主窗口。")?;
        // SAFETY: the live Tauri main window owns this native handle.
        unsafe {
            platform::hide_for_capture(hwnd.0);
        }
        let image = platform::capture(&monitor)?;
        let png = Arc::new(image.png()?);
        let info = {
            let mut data = state.lock();
            data.next_id += 1;
            let info = SessionInfo {
                id: data.next_id,
                width: image.width,
                height: image.height,
                mode,
            };
            data.capture = Some(Capture {
                info: info.clone(),
                image,
                png,
                background,
            });
            info
        };
        app.emit_to("overlay", "capture-started", info)
            .map_err(|_| "无法加载选区窗口，请重试。")?;
        Ok(())
    })();
    if result.is_err() {
        let mut data = state.lock();
        data.capturing = false;
        data.capture = None;
        drop(data);
        let _ = overlay.hide();
        show_main(app);
    }
    result
}

#[tauri::command]
fn start_capture(
    app: AppHandle,
    window: WebviewWindow,
    monitor_id: Option<String>,
) -> Result<(), String> {
    require(&window, "main")?;
    begin_capture(&app, monitor_id)
}

#[tauri::command]
fn current_session(app: AppHandle, window: WebviewWindow) -> Result<Option<SessionInfo>, String> {
    require(&window, "overlay")?;
    Ok(app
        .state::<AppState>()
        .lock()
        .capture
        .as_ref()
        .map(|c| c.info.clone()))
}

#[tauri::command]
fn show_overlay(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "overlay")?;
    if !app
        .state::<AppState>()
        .lock()
        .capture
        .as_ref()
        .is_some_and(|c| c.info.id == id)
    {
        return Err("本次截图已结束。".into());
    }
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(|_| "无法显示选区窗口。".into())
}

fn cancel(app: &AppHandle, id: Option<u64>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let background = {
        let mut data = state.lock();
        let Some(capture) = data.capture.as_ref() else {
            return Ok(());
        };
        if id.is_some_and(|id| id != capture.info.id) {
            return Err("本次截图已结束。".into());
        }
        let background = capture.background && data.tray_available;
        data.capture = None;
        data.capturing = false;
        background
    };
    window(app, "overlay")?
        .hide()
        .map_err(|_| "无法关闭选区窗口。")?;
    if !background {
        show_main(app);
    }
    Ok(())
}

#[tauri::command]
fn cancel_capture(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "overlay")?;
    cancel(&app, Some(id))
}

#[tauri::command]
fn accept_capture(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    points: Vec<Point>,
    width: f64,
    height: f64,
) -> Result<Preview, String> {
    require(&window, "overlay")?;
    // The viewport is measured by WebView, rather than assuming a particular Windows scaling factor.
    let viewport = window.inner_size().map_err(|_| "无法读取选区窗口尺寸。")?;
    let scale = window.scale_factor().map_err(|_| "无法读取显示器缩放。")?;
    if ((width * scale) - viewport.width as f64).abs() > 2.
        || ((height * scale) - viewport.height as f64).abs() > 2.
    {
        return Err("显示器缩放已改变，请取消后重新截图。".into());
    }
    let state = app.state::<AppState>();
    let info = {
        let mut data = state.lock();
        let capture = data
            .capture
            .as_ref()
            .filter(|c| c.info.id == id)
            .ok_or("本次截图已结束。")?;
        let image = imaging::crop(&capture.image, &points, width, height)?;
        let png = Arc::new(image.png()?);
        let info = Preview {
            id,
            width: image.width,
            height: image.height,
        };
        data.result = Some(ResultImage {
            info: info.clone(),
            image,
            png,
        });
        data.capture = None;
        data.capturing = false;
        info
    };
    window.hide().map_err(|_| "无法隐藏选区窗口。")?;
    app.emit_to("main", "preview-changed", info.clone())
        .map_err(|_| "无法更新预览。")?;
    show_main(&app);
    Ok(info)
}

#[tauri::command]
fn set_mode(app: AppHandle, mode: Mode) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut data = state.lock();
    let mut settings = data.settings.clone();
    settings.mode = mode;
    settings::save(&data.settings_path, &settings)?;
    data.settings = settings;
    drop(data);
    let _ = app.emit_to("main", "mode-changed", mode);
    Ok(())
}

fn parse_shortcut(value: &str) -> Result<Shortcut, String> {
    value.parse().map_err(|_| "快捷键格式无效。".into())
}

#[tauri::command]
fn set_hotkey(app: AppHandle, window: WebviewWindow, hotkey: String) -> Result<String, String> {
    require(&window, "main")?;
    let normalized = settings::normalize_hotkey(&hotkey)?;
    let candidate = parse_shortcut(&normalized)?;
    let state = app.state::<AppState>();
    let mut data = state.lock();
    if data.active_hotkey.as_ref() == Some(&normalized) {
        return Ok(normalized);
    }
    // Reserve the new binding first. A conflict leaves the previous shortcut untouched.
    app.global_shortcut().register(candidate).map_err(|_| {
        format!(
            "{normalized} 已被占用或无法注册。{}",
            data.active_hotkey
                .as_ref()
                .map(|key| format!(" 原快捷键 {key} 仍有效。"))
                .unwrap_or_default()
        )
    })?;
    let mut settings = data.settings.clone();
    settings.hotkey = normalized.clone();
    if let Err(error) = settings::save(&data.settings_path, &settings) {
        let _ = app.global_shortcut().unregister(candidate);
        return Err(format!("{error} 原快捷键保持不变。"));
    }
    if let Some(old) = &data.active_hotkey {
        if app
            .global_shortcut()
            .unregister(parse_shortcut(old)?)
            .is_err()
        {
            let _ = app.global_shortcut().unregister(candidate);
            let _ = settings::save(&data.settings_path, &data.settings);
            return Err("无法注销旧快捷键，已保留原设置，请重试。".into());
        }
    }
    data.settings = settings;
    data.active_hotkey = Some(normalized.clone());
    data.hotkey_error = None;
    drop(data);
    if let Some(tray) = app.tray_by_id("freecut") {
        let _ = tray.set_tooltip(Some(format!("FreeCut · {normalized} 截图")));
    }
    Ok(normalized)
}

struct ModalGuard(AppHandle);
impl Drop for ModalGuard {
    fn drop(&mut self) {
        self.0.state::<AppState>().lock().modal = false;
    }
}
fn enter_modal(app: &AppHandle) -> Result<ModalGuard, String> {
    let state = app.state::<AppState>();
    let mut data = state.lock();
    if data.modal || data.capturing {
        return Err("请先完成当前操作。".into());
    }
    data.modal = true;
    Ok(ModalGuard(app.clone()))
}

#[tauri::command]
fn copy_image(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "main")?;
    let state = app.state::<AppState>();
    let data = state.lock();
    let result = data.result.as_ref().ok_or("还没有可复制的截图。")?;
    // SAFETY: this handle belongs to the live Tauri command window.
    unsafe {
        platform::copy_image(
            window.hwnd().map_err(|_| "无法访问主窗口。")?.0,
            &result.image,
            &result.png,
        )
    }
}

#[tauri::command]
fn save_image(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    require(&window, "main")?;
    let _guard = enter_modal(&app)?;
    let image = app
        .state::<AppState>()
        .lock()
        .result
        .as_ref()
        .map(|r| r.image.clone())
        .ok_or("还没有可保存的截图。")?;
    let filename = format!(
        "FreeCut-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let path = rfd::FileDialog::new()
        .set_parent(&window)
        .set_title("保存透明 PNG")
        .add_filter("PNG 图像", &["png"])
        .set_file_name(filename)
        .save_file();
    path.map(|path| {
        imaging::save_png(&path, &image).map(|path| path.to_string_lossy().into_owned())
    })
    .transpose()
}

#[tauri::command]
fn hide_to_tray(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "main")?;
    if !app.state::<AppState>().lock().tray_available {
        return Err("当前桌面没有可用托盘。".into());
    }
    window.hide().map_err(|_| "无法隐藏主窗口。".into())
}

#[tauri::command]
fn quit_app(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "main")?;
    let _ = app.global_shortcut().unregister_all();
    app.exit(0);
    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let capture = MenuItem::with_id(app, "capture", "开始截图", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 FreeCut", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&capture, &show, &separator, &quit])?;
    let mut tray = TrayIconBuilder::with_id("freecut")
        .menu(&menu)
        .tooltip("FreeCut · 自由形状截图")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "capture" => {
                if let Err(error) = begin_capture(app, None) {
                    report_error(app, error);
                }
            }
            "show" => show_main(app),
            "quit" => {
                let _ = app.global_shortcut().unregister_all();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--background") {
                show_main(app);
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let active = app.state::<AppState>().lock().active_hotkey.clone();
                    if !active
                        .and_then(|key| parse_shortcut(&key).ok())
                        .is_some_and(|current| current == *shortcut)
                    {
                        return;
                    }
                    let app = app.clone();
                    let scheduled = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Err(error) = begin_capture(&scheduled, None) {
                            report_error(&scheduled, error);
                        }
                    });
                })
                .build(),
        )
        .register_uri_scheme_protocol("freecut", |context, request| {
            let state = context.app_handle().state::<AppState>();
            let data = state.lock();
            let path = request.uri().path().trim_start_matches('/');
            let bytes = path.split_once('-').and_then(|(kind, id)| {
                id.parse::<u64>().ok().and_then(|id| match kind {
                    "source" => data
                        .capture
                        .as_ref()
                        .filter(|c| c.info.id == id)
                        .map(|c| c.png.clone()),
                    "preview" => data
                        .result
                        .as_ref()
                        .filter(|r| r.info.id == id)
                        .map(|r| r.png.clone()),
                    _ => None,
                })
            });
            let (status, bytes) = bytes
                .map(|bytes| (200, bytes.as_ref().clone()))
                .unwrap_or((404, Vec::new()));
            tauri::http::Response::builder()
                .status(status)
                .header("Content-Type", "image/png")
                .header("Cache-Control", "no-store")
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap()
        })
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::load(&path);
            app.manage(AppState(Mutex::new(Data {
                settings: settings.clone(),
                settings_path: path,
                active_hotkey: None,
                hotkey_error: None,
                tray_available: false,
                capturing: false,
                modal: false,
                next_id: 0,
                capture: None,
                result: None,
            })));
            let tray_available = build_tray(app.handle()).is_ok();
            let registered = parse_shortcut(&settings.hotkey).and_then(|shortcut| {
                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|_| format!("{} 已被占用，请更换快捷键。", settings.hotkey))
            });
            let state = app.state::<AppState>();
            let mut data = state.lock();
            data.tray_available = tray_available;
            match registered {
                Ok(()) => data.active_hotkey = Some(settings.hotkey),
                Err(error) => data.hotkey_error = Some(error),
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "overlay" {
                    api.prevent_close();
                    let _ = cancel(window.app_handle(), None);
                } else if window
                    .app_handle()
                    .state::<AppState>()
                    .lock()
                    .tray_available
                {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            initial_state,
            frontend_ready,
            list_monitors,
            start_capture,
            current_session,
            show_overlay,
            cancel_capture,
            accept_capture,
            set_mode,
            set_hotkey,
            copy_image,
            save_image,
            hide_to_tray,
            quit_app
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 FreeCut")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let _ = app.global_shortcut().unregister_all();
            }
        });
}
