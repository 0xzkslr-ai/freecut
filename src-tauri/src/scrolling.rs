use super::*;
use crate::stitching::{Region, Stitcher};
use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScrollInfo {
    id: u64,
    width: u32,
    height: u32,
    frames: u32,
    paused: bool,
    message: String,
    hotkey: String,
}

struct Running {
    stitcher: Option<Stitcher>,
    region: Monitor,
    monitor: Monitor,
    info: ScrollInfo,
    safe_position: Option<(i32, i32)>,
    panel_size: PhysicalSize<u32>,
    background: bool,
}

pub(super) struct Job {
    stopped: AtomicBool,
    revision: AtomicU64,
    running: Mutex<Running>,
}

impl Job {
    fn lock(&self) -> MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn job(app: &AppHandle) -> Result<Arc<Job>, String> {
    app.state::<AppState>()
        .lock()
        .scroll
        .clone()
        .ok_or("当前没有滚动截图。".into())
}

pub(super) fn active(app: &AppHandle) -> bool {
    app.state::<AppState>().lock().scroll.is_some()
}

pub(super) fn stop(app: &AppHandle) {
    if let Ok(job) = job(app) {
        job.stopped.store(true, Ordering::Release);
    }
}

fn publish(app: &AppHandle, info: &ScrollInfo) {
    let Ok(job) = job(app) else {
        return;
    };
    let revision = job.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let current = info.clone();
    let scheduled = app.clone();
    // Tray operations wait for the UI thread. Never call them from a worker while
    // holding its mutex: finishing on the UI thread also needs that mutex.
    let _ = app.run_on_main_thread(move || {
        if job.stopped.load(Ordering::Acquire) || job.revision.load(Ordering::Acquire) != revision {
            return;
        }
        let _ = scheduled.emit_to("scroll", "scroll-progress", &current);
        if let Some(tray) = scheduled.tray_by_id("freecut") {
            let _ = tray.set_tooltip(Some(format!(
                "FreeCut · {} × {} · {} · {} 结束",
                current.width,
                current.height,
                if current.paused {
                    "已暂停"
                } else {
                    "滚动采集中"
                },
                current.hotkey
            )));
        }
    });
}

fn restore_tooltip(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("freecut") {
        let key = app.state::<AppState>().lock().settings.hotkey.clone();
        let _ = tray.set_tooltip(Some(format!("FreeCut · {key} 截图")));
    }
}

fn controls_position(
    monitor: &Monitor,
    region: &Monitor,
    size: PhysicalSize<u32>,
) -> Option<(i32, i32)> {
    let (w, h) = (size.width as i32, size.height as i32);
    let (right, bottom) = (
        monitor.x + monitor.width as i32,
        monitor.y + monitor.height as i32,
    );
    let candidates = [
        (monitor.x + 8, monitor.y + 8),
        (right - w - 8, monitor.y + 8),
        (monitor.x + 8, bottom - h - 8),
        (right - w - 8, bottom - h - 8),
    ];
    candidates.into_iter().find(|&(x, y)| {
        x >= monitor.x
            && y >= monitor.y
            && x + w <= right
            && y + h <= bottom
            && (x + w <= region.x
                || x >= region.x + region.width as i32
                || y + h <= region.y
                || y >= region.y + region.height as i32)
    })
}

// Never place running controls over the sampled rectangle. When no free space exists,
// the global shortcut and tray menu remain available; paused controls may overlap safely.
pub(super) fn show_controls(app: &AppHandle) {
    let Ok(job) = job(app) else {
        return;
    };
    let mut running = job.lock();
    running.info.paused = true;
    running.info.message = "已暂停。可以调整地图位置后继续，或结束并预览。".into();
    if let Ok(panel) = window(app, "scroll") {
        let (x, y) = running
            .safe_position
            .unwrap_or((running.monitor.x + 8, running.monitor.y + 8));
        let _ = panel.set_position(PhysicalPosition::new(x, y));
        let _ = panel.set_size(running.panel_size);
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    publish(app, &running.info);
}

#[tauri::command]
pub(super) fn start_scroll(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    points: Vec<Point>,
    width: f64,
    height: f64,
) -> Result<(), String> {
    require(&window, "overlay")?;
    validate_viewport(&window, width, height)?;
    let panel = self::window(&app, "scroll")?;
    let scale = window.scale_factor().map_err(|_| "无法读取显示器缩放。")?;
    let panel_size = PhysicalSize::new((440. * scale).ceil() as u32, (260. * scale).ceil() as u32);
    let (stitcher, region, monitor, background, hotkey, safe_position) = {
        let state = app.state::<AppState>();
        let data = state.lock();
        let capture = data
            .capture
            .as_ref()
            .filter(|c| c.info.id == id && !c.info.crop)
            .ok_or("本次截图已结束，或当前正在裁剪图片。")?;
        let monitor = capture.monitor.clone().ok_or("无法读取采集显示器。")?;
        let rect = Region::from_selection(&capture.image, &points, width, height)?;
        let region = Monitor {
            x: monitor.x + rect.x as i32,
            y: monitor.y + rect.y as i32,
            width: rect.width,
            height: rect.height,
            ..monitor.clone()
        };
        let safe_position = controls_position(&monitor, &region, panel_size);
        if safe_position.is_none() && !data.tray_available && data.active_hotkey.is_none() {
            return Err("没有可用的结束快捷键或托盘，请缩小选区以留出控制窗口空间。".into());
        }
        (
            Stitcher::new(rect.extract(&capture.image)?)?,
            region,
            monitor,
            capture.background,
            data.active_hotkey
                .clone()
                .unwrap_or_else(|| "托盘菜单".into()),
            safe_position,
        )
    };
    window.hide().map_err(|_| "无法隐藏选区窗口。")?;
    // SAFETY: this live overlay owns the handle; flush the compositor before sampling.
    unsafe {
        platform::hide_for_capture(window.hwnd().map_err(|_| "无法访问选区窗口。")?.0);
    }
    let info = ScrollInfo {
        id,
        width: stitcher.image().width,
        height: stitcher.image().height,
        frames: 1,
        paused: false,
        message: "请保持缩放不变，缓慢移动地图；移动时保留至少一半重叠。".into(),
        hotkey,
    };
    let job = Arc::new(Job {
        stopped: AtomicBool::new(false),
        revision: AtomicU64::new(0),
        running: Mutex::new(Running {
            stitcher: Some(stitcher),
            region,
            monitor,
            info: info.clone(),
            safe_position,
            panel_size,
            background,
        }),
    });
    {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.capture = None;
        data.scroll = Some(job.clone());
    }
    if let Some((x, y)) = safe_position {
        let _ = panel.set_position(PhysicalPosition::new(x, y));
        let _ = panel.set_size(panel_size);
        let _ = panel.show();
    }
    publish(&app, &info);
    let worker_app = app.clone();
    if std::thread::Builder::new()
        .name("freecut-scroll".into())
        .spawn(move || sample(worker_app, job))
        .is_err()
    {
        let _ = finish(&app);
        return Err("无法启动连续采集线程，请重试。".into());
    }
    Ok(())
}

fn sample(app: AppHandle, job: Arc<Job>) {
    // The first frame is the original frozen capture, never the selection overlay.
    // Begin live sampling at the next regular frame interval after hiding the overlay.
    std::thread::sleep(Duration::from_millis(140));
    while !job.stopped.load(Ordering::Acquire) {
        let mut failed = false;
        {
            let mut running = job.lock();
            if job.stopped.load(Ordering::Acquire) {
                break;
            }
            if !running.info.paused {
                let result = platform::capture(&running.region)
                    .and_then(|image| running.stitcher.as_mut().ok_or("采集已结束。")?.push(image));
                match result {
                    Ok(changed) => {
                        if changed {
                            let stitcher = running.stitcher.as_ref().unwrap();
                            let (width, height, frames) = (
                                stitcher.image().width,
                                stitcher.image().height,
                                stitcher.frames(),
                            );
                            running.info.width = width;
                            running.info.height = height;
                            running.info.frames = frames;
                            running.info.message = "正在拼接；未经过的区域会保持透明。".into();
                            publish(&app, &running.info);
                        }
                    }
                    Err(error) => {
                        running.info.paused = true;
                        running.info.message = error;
                        publish(&app, &running.info);
                        failed = true;
                    }
                }
            }
        }
        if failed {
            let scheduled = app.clone();
            let expected = job.clone();
            let _ = app.run_on_main_thread(move || {
                // A queued error must not reopen controls after finish or resume.
                if expected.stopped.load(Ordering::Acquire) {
                    return;
                }
                let running = expected.lock();
                if !running.info.paused {
                    return;
                }
                if let Ok(panel) = window(&scheduled, "scroll") {
                    let (x, y) = running
                        .safe_position
                        .unwrap_or((running.monitor.x + 8, running.monitor.y + 8));
                    let _ = panel.set_position(PhysicalPosition::new(x, y));
                    let _ = panel.set_size(running.panel_size);
                    let _ = panel.show();
                    let _ = panel.set_focus();
                }
            });
        }
        std::thread::sleep(Duration::from_millis(140));
    }
}

#[tauri::command]
pub(super) fn scroll_state(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<ScrollInfo>, String> {
    require(&window, "scroll")?;
    Ok(job(&app).ok().map(|job| job.lock().info.clone()))
}

#[tauri::command]
pub(super) fn pause_scroll(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    paused: bool,
) -> Result<(), String> {
    require(&window, "scroll")?;
    let job = job(&app)?;
    let mut running = job.lock();
    if running.info.id != id || job.stopped.load(Ordering::Acquire) {
        return Err("本次滚动截图已结束。".into());
    }
    if !paused && running.safe_position.is_none() {
        window.hide().map_err(|_| "无法隐藏控制窗口。")?;
        // SAFETY: hide and flush before the worker can resume capture.
        unsafe {
            platform::hide_for_capture(window.hwnd().map_err(|_| "无法访问控制窗口。")?.0);
        }
    }
    running.info.paused = paused;
    running.info.message = if paused {
        "已暂停。可以调整地图位置后继续，或结束并预览。"
    } else {
        "继续采集。请从上次成功位置附近缓慢移动，保持重叠。"
    }
    .into();
    publish(&app, &running.info);
    Ok(())
}

pub(super) fn finish(app: &AppHandle) -> Result<(), String> {
    let Ok(job) = job(app) else {
        return Ok(());
    };
    let mut running = job.lock();
    let stitcher = running.stitcher.as_ref().ok_or("本次滚动截图已结束。")?;
    let png = match stitcher.image().png() {
        Ok(png) => Arc::new(png),
        Err(error) => {
            running.info.paused = true;
            running.info.message = error.clone();
            publish(app, &running.info);
            return Err(error);
        }
    };
    job.stopped.store(true, Ordering::Release);
    let image = running.stitcher.take().unwrap().into_image();
    let info = Preview {
        id: running.info.id,
        width: image.width,
        height: image.height,
    };
    drop(running);
    {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.result = Some(ResultImage {
            info: info.clone(),
            image,
            png,
        });
        data.scroll = None;
        data.capturing = false;
    }
    if let Ok(panel) = window(app, "scroll") {
        let _ = panel.hide();
    }
    restore_tooltip(app);
    let _ = app.emit_to("scroll", "scroll-ended", ());
    let _ = app.emit_to("main", "preview-changed", info);
    show_main(app);
    Ok(())
}

#[tauri::command]
pub(super) fn finish_scroll(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "scroll")?;
    if job(&app)?.lock().info.id != id {
        return Err("本次滚动截图已结束。".into());
    }
    finish(&app)
}

#[tauri::command]
pub(super) fn cancel_scroll(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "scroll")?;
    let job = job(&app)?;
    let running = job.lock();
    if running.info.id != id {
        return Err("本次滚动截图已结束。".into());
    }
    job.stopped.store(true, Ordering::Release);
    let background = running.background;
    drop(running);
    {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.scroll = None;
        data.capturing = false;
    }
    window.hide().map_err(|_| "无法隐藏控制窗口。")?;
    restore_tooltip(&app);
    let _ = app.emit_to("scroll", "scroll-ended", ());
    if !background {
        show_main(&app);
    }
    Ok(())
}
