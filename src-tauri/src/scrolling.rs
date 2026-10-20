use super::*;
use crate::stitching::{self, Coverage, CoverageRect, Region, Stitcher};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

type CapturedFrame = (u64, Result<Image, String>);
type CaptureStream = mpsc::Receiver<CapturedFrame>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScrollInfo {
    id: u64,
    width: u32,
    height: u32,
    frames: u32,
    paused: bool,
    finishing: bool,
    recovering: bool,
    message: String,
    hotkey: String,
    coverage: Coverage,
}

pub(super) struct Job {
    stopped: AtomicBool,
    paused: AtomicBool,
    finishing: AtomicBool,
    generation: AtomicU64,
    revision: AtomicU64,
    info: Mutex<ScrollInfo>,
    region: Monitor,
    background: bool,
}

impl Job {
    fn info(&self) -> MutexGuard<'_, ScrollInfo> {
        self.info.lock().unwrap_or_else(|e| e.into_inner())
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

fn publish(app: &AppHandle, job: &Arc<Job>) {
    let revision = job.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let info = job.info().clone();
    let expected = job.clone();
    let scheduled = app.clone();
    let _ = app.run_on_main_thread(move || {
        if expected.stopped.load(Ordering::Acquire)
            || expected.revision.load(Ordering::Acquire) != revision
        {
            return;
        }
        let _ = scheduled.emit_to("scroll", "scroll-progress", &info);
        if let Some(tray) = scheduled.tray_by_id("freecut") {
            let status = if info.finishing {
                "正在合成"
            } else if info.paused {
                "已暂停"
            } else if info.recovering {
                "正在定位"
            } else {
                "正在采集"
            };
            let _ = tray.set_tooltip(Some(format!(
                "FreeCut · {} × {} · {status} · {} 结束",
                info.width, info.height, info.hotkey
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
    [
        (monitor.x + 8, monitor.y + 8),
        (right - w - 8, monitor.y + 8),
        (monitor.x + 8, bottom - h - 8),
        (right - w - 8, bottom - h - 8),
    ]
    .into_iter()
    .find(|&(x, y)| {
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

pub(super) fn show_controls(app: &AppHandle) {
    let Ok(job) = job(app) else {
        return;
    };
    if !job.finishing.load(Ordering::Acquire) {
        job.paused.store(true, Ordering::Release);
        job.generation.fetch_add(1, Ordering::AcqRel);
        let mut info = job.info();
        info.paused = true;
        info.message = "已暂停。覆盖图中的空白尚未采集；可以继续移动后补齐。".into();
    }
    if let Ok(panel) = window(app, "scroll") {
        let _ = panel.set_always_on_top(true);
        let _ = panel.unminimize();
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    publish(app, &job);
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
    let (image, region, background, hotkey, position, panel_size) = {
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
        let panel_size = PhysicalSize::new(
            ((440. * scale).ceil() as u32).min(monitor.width),
            ((374. * scale).ceil() as u32).min(monitor.height),
        );
        // Prefer a corner outside the selected region. Capture exclusion also
        // allows the panel to stay visible when the selection fills the screen.
        let position = controls_position(&monitor, &region, panel_size).unwrap_or((
            monitor.x + monitor.width.saturating_sub(panel_size.width + 8) as i32,
            monitor.y + 8.min(monitor.height.saturating_sub(panel_size.height)) as i32,
        ));
        (
            rect.extract(&capture.image)?,
            region,
            capture.background,
            data.active_hotkey
                .clone()
                .unwrap_or_else(|| "托盘菜单".into()),
            position,
            panel_size,
        )
    };
    // Configure and verify exclusion before leaving selection mode. An OS error
    // must not silently burn the controls (or a black rectangle) into the map.
    unsafe {
        platform::exclude_from_capture(panel.hwnd().map_err(|_| "无法访问控制窗口。")?.0)?;
    }
    panel
        .set_position(PhysicalPosition::new(position.0, position.1))
        .and_then(|_| panel.set_size(panel_size))
        .and_then(|_| panel.set_always_on_top(true))
        .map_err(|_| "无法准备滚动截图控制窗口。")?;
    window.hide().map_err(|_| "无法隐藏选区窗口。")?;
    // SAFETY: this live application window owns the handle.
    unsafe {
        platform::hide_for_capture(window.hwnd().map_err(|_| "无法访问选区窗口。")?.0)?;
    }
    let rect = CoverageRect {
        x: 0,
        y: 0,
        width: image.width,
        height: image.height,
    };
    let info = ScrollInfo {
        id,
        width: image.width,
        height: image.height,
        frames: 1,
        paused: false,
        finishing: false,
        recovering: false,
        message: "正在准备采集。保持缩放不变，可以上下左右移动，也可以返回补齐。".into(),
        hotkey,
        coverage: Coverage {
            bounds: rect,
            viewport: rect,
            regions: vec![rect],
            unplaced: 0,
        },
    };
    let job = Arc::new(Job {
        stopped: AtomicBool::new(false),
        paused: AtomicBool::new(false),
        finishing: AtomicBool::new(false),
        generation: AtomicU64::new(0),
        revision: AtomicU64::new(0),
        info: Mutex::new(info),
        region,
        background,
    });
    {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.capture = None;
        data.scroll = Some(job.clone());
    }
    let worker_app = app.clone();
    let worker_job = job.clone();
    let started = panel
        .show()
        .map_err(|_| "无法显示滚动截图控制窗口。")
        .and_then(|_| {
            std::thread::Builder::new()
                .name("freecut-scroll".into())
                .spawn(move || sample(worker_app, worker_job, image))
                .map(|_| ())
                .map_err(|_| "无法启动采集线程，请重试。")
        });
    if let Err(error) = started {
        job.stopped.store(true, Ordering::Release);
        {
            let state = app.state::<AppState>();
            let mut data = state.lock();
            data.scroll = None;
            data.capturing = false;
        }
        let _ = panel.hide();
        show_main(&app);
        return Err(error.into());
    }
    publish(&app, &job);
    Ok(())
}

fn complete(app: &AppHandle, job: &Arc<Job>, image: Image, png: Arc<Vec<u8>>) {
    let info = Preview {
        id: job.info().id,
        width: image.width,
        height: image.height,
    };
    let unresolved = job.info().coverage.unplaced;
    let expected = job.clone();
    let scheduled = app.clone();
    let _ = app.run_on_main_thread(move || {
        if expected.stopped.load(Ordering::Acquire) { return; }
        {
            let state = scheduled.state::<AppState>();
            let mut data = state.lock();
            if !data.scroll.as_ref().is_some_and(|current| Arc::ptr_eq(current, &expected)) { return; }
            expected.stopped.store(true, Ordering::Release);
            data.result = Some(ResultImage { info: info.clone(), image, png });
            data.scroll = None;
            data.capturing = false;
        }
        if let Ok(panel) = window(&scheduled, "scroll") { let _ = panel.hide(); }
        restore_tooltip(&scheduled);
        let _ = scheduled.emit_to("scroll", "scroll-ended", ());
        let _ = scheduled.emit_to("main", "preview-changed", info);
        show_main(&scheduled);
        if unresolved > 0 {
            let _ = scheduled.emit_to("main", "capture-error", format!("已导出成功定位的区域；另有 {unresolved} 个缓存片段未能定位，缺失区域保持透明。"));
        }
    });
}

fn capture_stream(job: Arc<Job>) -> Result<CaptureStream, String> {
    let bytes = job.region.width as usize * job.region.height as usize * 4;
    let capacity = (48 * 1024 * 1024 / bytes.max(1)).clamp(1, 3);
    let (sender, receiver) = mpsc::sync_channel(capacity);
    std::thread::Builder::new()
        .name("freecut-screen-sampler".into())
        .spawn(move || {
            // The frozen image supplies the first frame; wait only for the first
            // regular live sample while initial keyframe encoding runs concurrently.
            std::thread::sleep(Duration::from_millis(60));
            while !job.stopped.load(Ordering::Acquire) {
                if job.paused.load(Ordering::Acquire) || job.finishing.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(30));
                    continue;
                }
                let started = Instant::now();
                let generation = job.generation.load(Ordering::Acquire);
                let image = platform::capture(&job.region);
                if job.generation.load(Ordering::Acquire) == generation
                    && !job.paused.load(Ordering::Acquire)
                    && !job.finishing.load(Ordering::Acquire)
                {
                    match sender.try_send((generation, image)) {
                        Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
                if let Some(wait) = Duration::from_millis(50).checked_sub(started.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        })
        .map_err(|_| "无法创建连续采集线程，请结束当前截图后重试。")?;
    Ok(receiver)
}

fn sample(app: AppHandle, job: Arc<Job>, image: Image) {
    let receiver = match capture_stream(job.clone()) {
        Ok(receiver) => Some(receiver),
        Err(error) => {
            job.paused.store(true, Ordering::Release);
            {
                let mut info = job.info();
                info.paused = true;
                info.message = error;
            }
            publish(&app, &job);
            None
        }
    };
    let mut stitcher = match Stitcher::new(image) {
        Ok(stitcher) => stitcher,
        Err(error) => {
            let scheduled = app.clone();
            let expected = job.clone();
            let _ = app.run_on_main_thread(move || {
                if expected.stopped.load(Ordering::Acquire) {
                    return;
                }
                {
                    let state = scheduled.state::<AppState>();
                    let mut data = state.lock();
                    data.scroll = None;
                    data.capturing = false;
                }
                expected.stopped.store(true, Ordering::Release);
                if let Ok(panel) = window(&scheduled, "scroll") {
                    let _ = panel.hide();
                }
                restore_tooltip(&scheduled);
                report_error(&scheduled, error);
            });
            return;
        }
    };
    let mut last_publish = Instant::now() - Duration::from_secs(1);
    while !job.stopped.load(Ordering::Acquire) {
        if job.finishing.load(Ordering::Acquire) {
            if let Some(receiver) = &receiver {
                for (generation, image) in receiver.try_iter().take(3) {
                    if generation == job.generation.load(Ordering::Acquire) {
                        if let Ok(image) = image {
                            let _ = stitcher.push(image);
                        }
                    }
                }
            }
            job.info().coverage = stitcher.coverage();
            let output = stitcher
                .finalize()
                .and_then(|image| image.png().map(|png| (image, Arc::new(png))));
            match output {
                Ok((image, png)) => {
                    complete(&app, &job, image, png);
                    return;
                }
                Err(error) => {
                    job.finishing.store(false, Ordering::Release);
                    job.paused.store(true, Ordering::Release);
                    {
                        let mut info = job.info();
                        info.finishing = false;
                        info.paused = true;
                        info.message = error;
                    }
                    publish(&app, &job);
                }
            }
        }
        if job.paused.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(30));
            continue;
        }
        let started = Instant::now();
        let generation = job.generation.load(Ordering::Acquire);
        let captured = if let Some(receiver) = &receiver {
            match receiver.recv_timeout(Duration::from_millis(80)) {
                Ok((captured_generation, image)) if captured_generation == generation => image,
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Err("连续采集线程已停止，请结束并保存当前图片。".into())
                }
            }
        } else {
            platform::capture(&job.region)
        };
        if job.stopped.load(Ordering::Acquire) {
            break;
        }
        if job.generation.load(Ordering::Acquire) != generation
            || job.finishing.load(Ordering::Acquire)
        {
            continue;
        }
        let result = captured.and_then(|image| stitcher.push(image));
        let coverage = stitcher.coverage();
        {
            let mut info = job.info();
            info.width = coverage.bounds.width;
            info.height = coverage.bounds.height;
            info.frames = stitcher.frames();
            info.coverage = coverage;
            if !job.paused.load(Ordering::Acquire) && !job.finishing.load(Ordering::Acquire) {
                info.paused = false;
                match result {
                    Ok(_) => {
                        info.recovering = false;
                        info.message = if info.coverage.unplaced == 0 {
                            "正在采集。绿色为已覆盖区域，线框为当前位置；可以返回补齐空白。".into()
                        } else {
                            format!(
                                "正在采集，另有 {} 个片段待定位；可以返回已经过的区域帮助恢复。",
                                info.coverage.unplaced
                            )
                        };
                    }
                    Err(error) if stitching::is_alignment_error(&error) => {
                        info.recovering = true;
                        info.message = error;
                    }
                    Err(error) => {
                        job.paused.store(true, Ordering::Release);
                        info.paused = true;
                        info.recovering = false;
                        info.message = error;
                    }
                }
            }
        }
        // State reads never wait for matching, PNG encoding, cache I/O, or final composition.
        if last_publish.elapsed() >= Duration::from_millis(200) {
            publish(&app, &job);
            last_publish = Instant::now();
        }
        if let Some(wait) = Duration::from_millis(50).checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
    }
}

#[tauri::command]
pub(super) fn scroll_state(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<ScrollInfo>, String> {
    require(&window, "scroll")?;
    Ok(job(&app).ok().map(|job| job.info().clone()))
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
    if job.info().id != id || job.finishing.load(Ordering::Acquire) {
        return Err("本次截图正在结束，请稍候。".into());
    }
    window
        .set_always_on_top(true)
        .map_err(|_| "无法置顶控制窗口。")?;
    job.generation.fetch_add(1, Ordering::AcqRel);
    job.paused.store(paused, Ordering::Release);
    {
        let mut info = job.info();
        info.paused = paused;
        info.message = if paused {
            "已暂停，可以继续移动后补齐。"
        } else {
            "正在继续采集和定位；可以上下左右移动或返回旧区域。"
        }
        .into();
    }
    publish(&app, &job);
    Ok(())
}

pub(super) fn finish(app: &AppHandle) -> Result<(), String> {
    let Ok(job) = job(app) else {
        return Ok(());
    };
    if job.finishing.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    job.paused.store(true, Ordering::Release);
    {
        let mut info = job.info();
        info.finishing = true;
        info.paused = true;
        info.message = "正在校正位置并合成原始像素，完成后会打开预览。".into();
    }
    publish(app, &job);
    Ok(())
}

#[tauri::command]
pub(super) fn finish_scroll(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "scroll")?;
    if job(&app)?.info().id != id {
        return Err("本次滚动截图已结束。".into());
    }
    finish(&app)
}

#[tauri::command]
pub(super) fn cancel_scroll(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    require(&window, "scroll")?;
    let job = job(&app)?;
    if job.info().id != id {
        return Err("本次滚动截图已结束。".into());
    }
    job.stopped.store(true, Ordering::Release);
    {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.scroll = None;
        data.capturing = false;
    }
    window.hide().map_err(|_| "无法隐藏控制窗口。")?;
    restore_tooltip(&app);
    let _ = app.emit_to("scroll", "scroll-ended", ());
    if !job.background {
        show_main(&app);
    }
    Ok(())
}
