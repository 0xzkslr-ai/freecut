use crate::{
    imaging::{self, Image, Point, Preview},
    platform::{self, Monitor},
    settings::{self, Mode, Settings},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, MutexGuard},
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[path = "diagnostics.rs"]
mod diagnostics;
#[path = "scrolling.rs"]
mod scrolling;

#[derive(Clone, Serialize)]
struct SessionInfo {
    id: u64,
    width: u32,
    height: u32,
    mode: Mode,
    crop: bool,
}
struct Capture {
    info: SessionInfo,
    image: Image,
    png: Arc<Vec<u8>>,
    background: bool,
    monitor: Option<Monitor>,
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
    scroll: Option<Arc<scrolling::Job>>,
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
    diagnostics::write("INFO", "Main frontend requesting initial state");
    let monitors = platform::monitors().unwrap_or_else(|error| {
        diagnostics::write("WARN", &format!("Monitor enumeration failed: {error}"));
        Vec::new()
    });
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
        let capturing = app.state::<AppState>().lock().capturing;
        if !capturing {
            window
                .unminimize()
                .map_err(|error| format!("无法恢复主窗口：{error}"))?;
            window
                .show()
                .map_err(|error| format!("无法显示主窗口：{error}"))?;
            if let Err(error) = window.set_focus() {
                diagnostics::write("WARN", &format!("Cannot focus main window: {error}"));
            }
            diagnostics::write("INFO", "Main window shown");
        }
    } else {
        diagnostics::write("INFO", "Background startup; main window stays hidden");
    }
    diagnostics::ready();
    Ok(())
}

#[tauri::command]
async fn frontend_failed(window: WebviewWindow, message: String) -> Result<(), String> {
    let label = window.label().to_owned();
    if !matches!(label.as_str(), "main" | "overlay" | "scroll") {
        return Err("当前窗口无法报告界面错误。".into());
    }
    // Preserve the error even if showing the failed window stalls on the UI thread.
    diagnostics::write("ERROR", &format!("Frontend {label}: {message}"));
    if label == "main" {
        let _ = window.show();
        let _ = window.set_focus();
    }
    tauri::async_runtime::spawn_blocking(move || {
        diagnostics::failure(
            "FreeCut 界面初始化或运行失败",
            &format!("窗口：{label}\n{message}"),
        );
    })
    .await
    .map_err(|_| "无法显示界面错误提示。".to_string())
}

#[tauri::command]
fn list_monitors() -> Result<Vec<Monitor>, String> {
    platform::monitors()
}

fn prepare_capture_windows(app: &AppHandle, monitor: &Monitor) -> Result<(), String> {
    let scheduled = app.clone();
    let monitor = monitor.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let main = window(&scheduled, "main")?;
            let overlay = window(&scheduled, "overlay")?;
            overlay.hide().map_err(|_| "无法隐藏选区窗口。")?;
            // Window mutations execute together on their owning thread. The worker
            // cannot read desktop pixels until this closure has finished.
            overlay
                .set_position(PhysicalPosition::new(monitor.x, monitor.y))
                .map_err(|_| "无法定位选区窗口。")?;
            overlay
                .set_size(PhysicalSize::new(monitor.width, monitor.height))
                .map_err(|_| "无法设置选区窗口尺寸。")?;
            main.minimize().map_err(|_| "无法最小化主窗口。")?;
            main.hide().map_err(|_| "无法隐藏主窗口。")?;
            let hwnd = main.hwnd().map_err(|_| "无法访问主窗口。")?;
            // SAFETY: this live Tauri main window owns the native handle.
            unsafe { platform::hide_for_capture(hwnd.0) }
        })();
        let _ = sender.send(result);
    })
    .map_err(|_| "无法安排截图窗口切换，请重试。".to_string())?;
    // Only the capture worker waits here; no app state lock is held.
    receiver
        .recv()
        .map_err(|_| "截图窗口切换已中断，请重试。".to_string())?
}

fn begin_capture(app: &AppHandle, monitor_id: Option<String>) -> Result<(), String> {
    let main = window(app, "main")?;
    let overlay = window(app, "overlay")?;
    // Native window queries can dispatch to the event loop. Do not hold app
    // state while waiting for them from the capture worker.
    let main_in_background =
        !main.is_visible().unwrap_or(true) || main.is_minimized().unwrap_or(false);
    let state = app.state::<AppState>();
    let (mode, background) = {
        let mut data = state.lock();
        if data.capturing || data.modal {
            return Ok(());
        }
        let background = data.tray_available && main_in_background;
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
        let hwnd = main.hwnd().map_err(|_| "无法访问主窗口。")?;
        // Exclude before changing visibility so a stale compositor surface cannot
        // put the main window into the frozen image. Restore on every exit path.
        // SAFETY: the main application window outlives this synchronous worker scope.
        let exclusion = unsafe { platform::CaptureExclusion::new(hwnd.0)? };
        prepare_capture_windows(app, &monitor)?;
        let image = platform::capture(&monitor)?;
        drop(exclusion);
        let png = Arc::new(image.png()?);
        let info = {
            let mut data = state.lock();
            data.next_id += 1;
            let info = SessionInfo {
                id: data.next_id,
                width: image.width,
                height: image.height,
                mode,
                crop: false,
            };
            data.capture = Some(Capture {
                info: info.clone(),
                image,
                png,
                background,
                monitor: Some(monitor),
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
async fn start_capture(
    app: AppHandle,
    window: WebviewWindow,
    monitor_id: Option<String>,
) -> Result<(), String> {
    require(&window, "main")?;
    capture_in_background(app, monitor_id).await
}

async fn capture_in_background(app: AppHandle, monitor_id: Option<String>) -> Result<(), String> {
    // Keep the event loop free to apply window visibility changes before the
    // desktop is read, and keep PNG encoding off the UI thread as well.
    tauri::async_runtime::spawn_blocking(move || begin_capture(&app, monitor_id))
        .await
        .map_err(|_| "截图任务意外停止，请重试。".to_string())?
}

fn request_capture(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = capture_in_background(app.clone(), None).await {
            let scheduled = app.clone();
            let _ = app.run_on_main_thread(move || report_error(&scheduled, error));
        }
    });
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
    validate_viewport(&window, width, height)?;
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

fn validate_viewport(window: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let viewport = window.inner_size().map_err(|_| "无法读取选区窗口尺寸。")?;
    let scale = window.scale_factor().map_err(|_| "无法读取显示器缩放。")?;
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.
        || height <= 0.
        || ((width * scale) - viewport.width as f64).abs() > 2.
        || ((height * scale) - viewport.height as f64).abs() > 2.
    {
        return Err("显示器缩放已改变，请取消后重新截图。".into());
    }
    Ok(())
}

#[tauri::command]
fn edit_result(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "main")?;
    let overlay = self::window(&app, "overlay")?;
    let monitor = platform::cursor_monitor(&platform::monitors()?)?;
    let state = app.state::<AppState>();
    let info = {
        let mut data = state.lock();
        if data.capturing || data.modal {
            return Err("请先完成当前操作。".into());
        }
        let result = data.result.as_ref().ok_or("还没有可裁剪的截图。")?;
        let image = result.image.clone();
        let png = result.png.clone();
        overlay
            .set_position(PhysicalPosition::new(monitor.x, monitor.y))
            .map_err(|_| "无法定位裁剪窗口。")?;
        overlay
            .set_size(PhysicalSize::new(monitor.width, monitor.height))
            .map_err(|_| "无法设置裁剪窗口尺寸。")?;
        data.next_id += 1;
        let info = SessionInfo {
            id: data.next_id,
            width: image.width,
            height: image.height,
            mode: data.settings.mode,
            crop: true,
        };
        data.capture = Some(Capture {
            info: info.clone(),
            image,
            png,
            background: false,
            monitor: None,
        });
        data.capturing = true;
        info
    };
    if window.hide().is_err() || app.emit_to("overlay", "capture-started", info).is_err() {
        let _ = cancel(&app, None);
        return Err("无法打开裁剪窗口，请重试。".into());
    }
    Ok(())
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
    if data.capturing {
        return Err("请结束当前截图后再修改快捷键。".into());
    }
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
    let finish = MenuItem::with_id(app, "finish-scroll", "结束滚动截图", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 FreeCut", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&capture, &finish, &show, &separator, &quit])?;
    let mut tray = TrayIconBuilder::with_id("freecut")
        .menu(&menu)
        .tooltip("FreeCut · 自由形状截图")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "capture" => {
                request_capture(app);
            }
            "show" => {
                if scrolling::active(app) {
                    scrolling::show_controls(app);
                } else {
                    show_main(app);
                }
            }
            "finish-scroll" => {
                if let Err(error) = scrolling::finish(app) {
                    report_error(app, error);
                }
            }
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
                if scrolling::active(tray.app_handle()) {
                    scrolling::show_controls(tray.app_handle());
                } else {
                    show_main(tray.app_handle());
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    diagnostics::init();
    diagnostics::watch_startup();
    diagnostics::write("INFO", "Checking WebView2 Runtime");
    match tauri::webview_version() {
        Ok(version) => diagnostics::write("INFO", &format!("WebView2 Runtime: {version}")),
        Err(error) => {
            diagnostics::failure(
                "FreeCut 无法启动",
                &format!(
                    "未检测到可用的 Microsoft WebView2 Runtime，或运行时不可用。\n请安装或修复 WebView2 后重新启动，也可使用 FreeCut 安装包安装所需运行时。\n官方下载：https://developer.microsoft.com/microsoft-edge/webview2/\n\n检测信息：{error}"
                ),
            );
            return;
        }
    }
    diagnostics::write("INFO", "Building Tauri application and windows");
    let built = tauri::Builder::default()
        // Tauri's IPC script has no trailing semicolon. Keep the scripts separate.
        .append_invoke_initialization_script(concat!("\n;", include_str!("startup.js")))
        .on_page_load(|webview, payload| {
            diagnostics::write(
                "INFO",
                &format!(
                    "Window {} page load: {:?}",
                    webview.label(),
                    payload.event()
                ),
            );
        })
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            diagnostics::write("INFO", "Second instance requested activation");
            if !args.iter().any(|arg| arg == "--background") {
                if scrolling::active(app) {
                    scrolling::show_controls(app);
                } else {
                    show_main(app);
                }
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
                        if scrolling::active(&scheduled) {
                            if let Err(error) = scrolling::finish(&scheduled) {
                                report_error(&scheduled, error);
                            }
                        } else {
                            request_capture(&scheduled);
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
            diagnostics::write("INFO", "Native setup started");
            let path = app.path().app_config_dir()?.join("settings.json");
            diagnostics::write("INFO", &format!("Loading settings: {}", path.display()));
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
                scroll: None,
            })));
            let tray_available = match build_tray(app.handle()) {
                Ok(()) => {
                    diagnostics::write("INFO", "System tray initialized");
                    true
                }
                Err(error) => {
                    diagnostics::write("WARN", &format!("System tray unavailable: {error}"));
                    false
                }
            };
            let registered = parse_shortcut(&settings.hotkey).and_then(|shortcut| {
                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|_| format!("{} 已被占用，请更换快捷键。", settings.hotkey))
            });
            let state = app.state::<AppState>();
            let mut data = state.lock();
            data.tray_available = tray_available;
            match registered {
                Ok(()) => {
                    diagnostics::write("INFO", "Global shortcut registered");
                    data.active_hotkey = Some(settings.hotkey);
                }
                Err(error) => {
                    diagnostics::write("WARN", &format!("Global shortcut unavailable: {error}"));
                    data.hotkey_error = Some(error);
                }
            }
            diagnostics::write("INFO", "Native setup complete; waiting for main frontend");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "scroll" {
                    api.prevent_close();
                    let _ = scrolling::finish(window.app_handle());
                } else if window.label() == "overlay" {
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
            frontend_failed,
            list_monitors,
            start_capture,
            current_session,
            show_overlay,
            cancel_capture,
            accept_capture,
            edit_result,
            scrolling::start_scroll,
            scrolling::scroll_state,
            scrolling::pause_scroll,
            scrolling::finish_scroll,
            scrolling::cancel_scroll,
            set_mode,
            set_hotkey,
            copy_image,
            save_image,
            hide_to_tray,
            quit_app
        ])
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|app, event| {
            if let tauri::RunEvent::Ready = event {
                diagnostics::write("INFO", "Tauri event loop ready");
            }
            if let tauri::RunEvent::Exit = event {
                diagnostics::stop();
                diagnostics::write("INFO", "Application exiting");
                scrolling::stop(app);
                let _ = app.global_shortcut().unregister_all();
            }
        }),
        Err(error) => {
            diagnostics::failure("FreeCut 启动失败", &format!("无法创建应用或窗口：{error}"))
        }
    }
    diagnostics::stop();
}
