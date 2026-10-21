use super::*;
use crate::{
    stitching::{CoverageRect, Region},
    video::{self, RecordingFormat, RecordingOptions, Timeline},
};
use std::{
    fs,
    io::{BufWriter, Read},
    sync::{
        atomic::{AtomicU64, Ordering},
        Condvar,
    },
    time::{Duration, Instant},
};
use tempfile::TempPath;
use windows::{
    core::{w, HSTRING},
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

#[path = "video_capture.rs"]
mod capture;
#[path = "video_encoder.rs"]
mod encoder;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordingInfo {
    id: u64,
    width: u32,
    height: u32,
    elapsed_ms: u64,
    frames: u64,
    paused: bool,
    starting: bool,
    finishing: bool,
    message: String,
    hotkey: String,
    region: CoverageRect,
    monitor_name: String,
    format: RecordingFormat,
    frame_rate: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordingResult {
    id: u64,
    width: u32,
    height: u32,
    elapsed_ms: u64,
    bytes: u64,
    path: String,
    warning: Option<String>,
    format: RecordingFormat,
}

enum RecordingEncoder {
    Mp4(encoder::VideoEncoder),
    Gif(crate::gif_recording::GifEncoder<BufWriter<fs::File>>),
}

impl RecordingEncoder {
    fn new(path: &std::path::Path, info: &RecordingInfo) -> Result<Self, String> {
        if info.format == RecordingFormat::Gif {
            let file = fs::File::create(path)
                .map_err(|error| format!("无法创建 GIF 录制文件：{error}"))?;
            return crate::gif_recording::GifEncoder::new(
                BufWriter::new(file),
                info.width,
                info.height,
            )
            .map(Self::Gif);
        }
        let encoder = match encoder::VideoEncoder::new(path, info.width, info.height, true) {
            Ok(encoder) => encoder,
            Err(error) => {
                diagnostics::write("WARN", &format!("Hardware encoder initialization failed; trying system software encoder: {error}"));
                let _ = fs::remove_file(path);
                encoder::VideoEncoder::new(path, info.width, info.height, false)?
            }
        };
        Ok(Self::Mp4(encoder))
    }

    fn write(&mut self, pixels: &[u8], timestamp: i64, duration: i64) -> Result<(), String> {
        match self {
            Self::Mp4(encoder) => encoder.write(pixels, timestamp, duration),
            Self::Gif(encoder) => encoder.write(pixels, timestamp, duration),
        }
    }

    fn finish(self) -> Result<(), String> {
        match self {
            Self::Mp4(encoder) => encoder.finish(),
            Self::Gif(encoder) => encoder.finish(),
        }
    }
}

pub(super) const GIF_PREVIEW_LIMIT: u64 = 20 * 1024 * 1024;

pub(super) fn gif_preview_path(app: &AppHandle, id: u64) -> Option<PathBuf> {
    app.state::<AppState>()
        .lock()
        .recorded_video
        .as_ref()
        .filter(|result| {
            result.id == id
                && result.format == RecordingFormat::Gif
                && result.bytes <= GIF_PREVIEW_LIMIT
        })
        .map(|result| PathBuf::from(&result.path))
}

pub(super) fn read_gif_preview(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "GIF 文件已被移动或无法读取。")?;
    let mut bytes = Vec::new();
    file.take(GIF_PREVIEW_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取 GIF 预览。")?;
    if bytes.len() as u64 > GIF_PREVIEW_LIMIT {
        return Err("GIF 较大，请打开文件查看。".into());
    }
    Ok(bytes)
}

struct Control {
    timeline: Option<Timeline>,
    paused: bool,
    finishing: bool,
    cancelled: bool,
    exit_after_finish: bool,
    frame_ready: bool,
    generation: u64,
    info: RecordingInfo,
}

pub(super) struct Job {
    control: Mutex<Control>,
    changed: Condvar,
    origin: Instant,
    revision: AtomicU64,
    region: Monitor,
    monitor: Monitor,
    destination: PathBuf,
    background: bool,
}

impl Job {
    fn lock(&self) -> MutexGuard<'_, Control> {
        self.control
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
    fn info(&self) -> RecordingInfo {
        let control = self.lock();
        let mut info = control.info.clone();
        info.elapsed_ms = control
            .timeline
            .as_ref()
            .map(|clock| clock.elapsed(self.origin.elapsed()).as_millis() as u64)
            .unwrap_or(0);
        info
    }
}

fn job(app: &AppHandle) -> Result<Arc<Job>, String> {
    app.state::<AppState>()
        .lock()
        .recording
        .clone()
        .ok_or("当前没有录屏。".into())
}

pub(super) fn active(app: &AppHandle) -> bool {
    app.state::<AppState>().lock().recording.is_some()
}

fn publish(app: &AppHandle, job: &Arc<Job>) {
    let revision = job.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let info = job.info();
    let expected = job.clone();
    let scheduled = app.clone();
    let _ = app.run_on_main_thread(move || {
        let current = scheduled.state::<AppState>().lock().recording.clone();
        if expected.revision.load(Ordering::Acquire) == revision
            && current.is_some_and(|current| Arc::ptr_eq(&current, &expected))
        {
            let _ = scheduled.emit_to("record", "recording-progress", &info);
            let _ = scheduled.emit_to("record-region", "recording-progress", &info);
            if let Some(tray) = scheduled.tray_by_id("freecut") {
                let status = if info.finishing {
                    "正在保存"
                } else if info.starting {
                    "准备录屏"
                } else if info.paused {
                    "录屏已暂停"
                } else {
                    "录屏中"
                };
                let _ =
                    tray.set_tooltip(Some(format!("FreeCut · {status} · {} 结束", info.hotkey)));
            }
        }
    });
}

pub(super) fn show_controls(app: &AppHandle) {
    if !active(app) {
        return;
    }
    let prepared = app.state::<AppState>().lock().recording_frame_ready;
    if prepared {
        if let Ok(frame) = window(app, "record-region") {
            let _ = frame.set_always_on_top(true);
            let _ = frame.show();
        }
    }
    if let Ok(panel) = window(app, "record") {
        let _ = panel.set_always_on_top(true);
        let _ = panel.unminimize();
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    if let Ok(job) = job(app) {
        publish(app, &job);
    }
}

fn region_frame(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(frame) = app.get_webview_window("record-region") {
        return Ok(frame);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "record-region")
        .ok_or("录制范围窗口配置不可用。")?;
    tauri::WebviewWindowBuilder::from_config(app, config)
        .and_then(|builder| builder.build())
        .map_err(|error| format!("无法创建录制范围窗口：{error}"))
}

#[tauri::command]
pub(super) fn recording_region_ready(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require(&window, "record-region")?;
    let current = {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.recording_frame_ready = true;
        data.recording.clone()
    };
    if let Some(job) = current {
        let show = {
            let control = job.lock();
            !control.cancelled && !control.finishing
        };
        if show {
            window
                .show()
                .map_err(|error| format!("无法显示录制范围：{error}"))?;
        }
        publish(&app, &job);
    }
    Ok(())
}

#[tauri::command]
pub(super) fn recording_region_shown(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), String> {
    require(&window, "record-region")?;
    let job = job(&app)?;
    if job.info().id != id {
        return Err("本次录屏已结束。".into());
    }
    if !window
        .is_visible()
        .map_err(|_| "无法读取录制范围窗口状态。")?
    {
        return Err("录制范围尚未显示。".into());
    }
    let size = window.inner_size().map_err(|_| "无法读取录制范围尺寸。")?;
    let position = window
        .inner_position()
        .map_err(|_| "无法读取录制范围位置。")?;
    if size.width != job.region.width
        || size.height != job.region.height
        || position.x != job.region.x
        || position.y != job.region.y
    {
        return Err("录制范围窗口与选区不一致，请重新选择。".into());
    }
    job.lock().frame_ready = true;
    job.changed.notify_all();
    Ok(())
}

#[tauri::command]
pub(super) fn highlight_recording_region(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), String> {
    require(&window, "record")?;
    let job = job(&app)?;
    if job.info().id != id {
        return Err("本次录屏已结束。".into());
    }
    let frame = self::window(&app, "record-region")?;
    frame
        .set_always_on_top(true)
        .and_then(|_| frame.show())
        .map_err(|error| format!("无法定位录制范围：{error}"))?;
    window
        .set_always_on_top(true)
        .map_err(|error| format!("无法置顶录屏控制窗：{error}"))?;
    app.emit_to("record-region", "recording-highlight", id)
        .map_err(|error| format!("无法标注录制范围：{error}"))
}

#[tauri::command]
pub(super) async fn start_recording(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    points: Vec<Point>,
    width: f64,
    height: f64,
    options: Option<RecordingOptions>,
) -> Result<bool, String> {
    require(&window, "overlay")?;
    validate_viewport(&window, width, height)?;
    let (region, monitor, background, hotkey, options) = {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        if data.modal || data.recording.is_some() || data.scroll.is_some() {
            return Err("请先结束当前操作。".into());
        }
        let capture = data
            .capture
            .as_ref()
            .filter(|capture| capture.info.id == id && !capture.info.crop)
            .ok_or("本次选区已结束。")?;
        let monitor = capture.monitor.clone().ok_or("无法读取录屏显示器。")?;
        let rect = Region::from_selection(&capture.image, &points, width, height)?;
        let options = capture
            .info
            .recording_options
            .or(options)
            .unwrap_or_default()
            .validate()?;
        options.output_size(rect.width, rect.height)?;
        let region = Monitor {
            x: monitor.x + rect.x as i32,
            y: monitor.y + rect.y as i32,
            width: rect.width,
            height: rect.height,
            ..monitor.clone()
        };
        let selected = (
            region,
            monitor,
            capture.background,
            data.active_hotkey
                .clone()
                .unwrap_or_else(|| "托盘菜单".into()),
            options,
        );
        data.modal = true;
        selected
    };
    // Reserve the native file dialog without holding any app lock across it.
    let _guard = ModalGuard(app.clone());
    let owner = window.clone();
    let destination = tauri::async_runtime::spawn_blocking(move || {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        rfd::FileDialog::new()
            .set_parent(&owner)
            .set_title(format!(
                "选择 {} 保存位置，然后开始录制",
                options.format.label()
            ))
            .add_filter(
                format!(
                    "{} {}",
                    options.format.label(),
                    if options.format == RecordingFormat::Gif {
                        "动图"
                    } else {
                        "视频"
                    }
                ),
                &[options.format.extension()],
            )
            .set_file_name(format!("FreeCut-{stamp}.{}", options.format.extension()))
            .save_file()
    })
    .await
    .map_err(|_| "无法选择录屏保存位置。".to_string())?;
    let Some(destination) = destination else {
        return Ok(false);
    };
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(options.format.extension()))
    {
        return Err(format!(
            "录制文件必须使用 .{} 扩展名，请重新选择保存位置。",
            options.format.extension()
        ));
    }
    // Confirm the selection is still live after the native dialog was dismissed.
    if !app
        .state::<AppState>()
        .lock()
        .capture
        .as_ref()
        .is_some_and(|capture| capture.info.id == id)
    {
        return Err("本次选区已结束，请重新框选录屏区域。".into());
    }
    if !platform::monitors()?.iter().any(|current| {
        current.id == monitor.id
            && current.x == monitor.x
            && current.y == monitor.y
            && current.width == monitor.width
            && current.height == monitor.height
    }) {
        return Err("显示器配置已改变，请取消后重新选择录屏区域。".into());
    }
    let parent = destination.parent().ok_or("保存目录无效。")?;
    let temporary = tempfile::Builder::new()
        .prefix(".freecut-record-")
        .suffix(&format!(".{}", options.format.extension()))
        .tempfile_in(parent)
        .map_err(|error| format!("无法在保存目录创建录屏文件：{error}"))?
        .into_temp_path();
    fs::remove_file(&temporary).map_err(|error| format!("无法准备录屏临时文件：{error}"))?;
    let panel = self::window(&app, "record")?;
    let frame = region_frame(&app)?;
    let scale = window.scale_factor().map_err(|_| "无法读取显示器缩放。")?;
    let panel_size = PhysicalSize::new(
        ((408. * scale).ceil() as u32).min(monitor.width),
        ((292. * scale).ceil() as u32).min(monitor.height),
    );
    let position = scrolling::controls_position(&monitor, &region, panel_size).unwrap_or((
        monitor.x + monitor.width.saturating_sub(panel_size.width + 8) as i32,
        monitor.y + 8.min(monitor.height.saturating_sub(panel_size.height)) as i32,
    ));
    // SAFETY: the control window stays alive for the recording session.
    unsafe {
        platform::exclude_from_capture(panel.hwnd().map_err(|_| "无法访问录屏控制窗口。")?.0)?;
        platform::exclude_from_capture(frame.hwnd().map_err(|_| "无法访问录制范围窗口。")?.0)?;
    }
    frame
        .set_focusable(false)
        .and_then(|_| frame.set_ignore_cursor_events(true))
        .and_then(|_| frame.set_position(PhysicalPosition::new(region.x, region.y)))
        .and_then(|_| frame.set_size(PhysicalSize::new(region.width, region.height)))
        .and_then(|_| frame.set_always_on_top(true))
        .map_err(|error| format!("无法设置录制范围窗口：{error}"))?;
    panel
        .set_position(PhysicalPosition::new(position.0, position.1))
        .and_then(|_| panel.set_size(panel_size))
        .and_then(|_| panel.set_always_on_top(true))
        .map_err(|error| format!("无法准备录屏控制窗口：{error}"))?;
    let (out_width, out_height) = options.output_size(region.width, region.height)?;
    let info = RecordingInfo {
        id,
        width: out_width,
        height: out_height,
        elapsed_ms: 0,
        frames: 0,
        paused: false,
        starting: true,
        finishing: false,
        message: format!(
            "正在准备录制… {} · {} FPS",
            options.format.label(),
            options.frame_rate()
        ),
        hotkey,
        region: CoverageRect {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        },
        monitor_name: monitor.name.clone(),
        format: options.format,
        frame_rate: options.frame_rate(),
    };
    let job = Arc::new(Job {
        control: Mutex::new(Control {
            timeline: None,
            paused: false,
            finishing: false,
            cancelled: false,
            exit_after_finish: false,
            frame_ready: false,
            generation: 0,
            info,
        }),
        changed: Condvar::new(),
        origin: Instant::now(),
        revision: AtomicU64::new(0),
        region,
        monitor,
        destination,
        background,
    });
    window.hide().map_err(|_| "无法隐藏选区窗口。")?;
    // SAFETY: this is the live overlay's native window handle.
    if let Err(error) = unsafe {
        platform::hide_for_capture(window.hwnd().map_err(|_| "无法访问选区窗口。")?.0)
    } {
        let _ = window.show();
        return Err(error);
    }
    let original_capture = {
        let state = app.state::<AppState>();
        let mut data = state.lock();
        data.recording = Some(job.clone());
        data.capture.take()
    };
    let prepared = (|| -> Result<(), String> {
        let prepared = app.state::<AppState>().lock().recording_frame_ready;
        if prepared {
            frame
                .show()
                .map_err(|error| format!("无法显示录制范围：{error}"))?;
        }
        panel
            .show()
            .map_err(|error| format!("无法显示录屏控制窗口：{error}"))
    })();
    if let Err(error) = prepared {
        {
            let state = app.state::<AppState>();
            let mut data = state.lock();
            data.recording = None;
            data.capture = original_capture;
        }
        let _ = frame.hide();
        let _ = window.show();
        return Err(error);
    }
    let worker_app = app.clone();
    let worker_job = job.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("freecut-record".into())
        .spawn(move || run(worker_app, worker_job, temporary))
    {
        {
            let state = app.state::<AppState>();
            let mut data = state.lock();
            data.recording = None;
            data.capture = original_capture;
        }
        let _ = panel.hide();
        let _ = frame.hide();
        let _ = window.show();
        return Err(format!("无法启动录屏线程：{error}"));
    }
    diagnostics::write("INFO", "Recording worker started");
    publish(&app, &job);
    Ok(true)
}

fn hns(duration: Duration) -> i64 {
    (duration.as_nanos() / 100).min(i64::MAX as u128) as i64
}

fn record(
    job: &Arc<Job>,
    temporary: &TempPath,
    app: &AppHandle,
) -> Result<Option<RecordingResult>, String> {
    // Wait for the visible frame's paint acknowledgement on this worker so the
    // event loop stays free and recording never starts without a marked region.
    {
        let control = job.lock();
        let (control, _) = job
            .changed
            .wait_timeout_while(control, Duration::from_secs(8), |control| {
                !control.frame_ready && !control.cancelled && !control.finishing
            })
            .unwrap_or_else(|error| error.into_inner());
        if control.cancelled || control.finishing {
            return Ok(None);
        }
        if !control.frame_ready {
            return Err("录制范围窗口未能就绪，请重新开始录屏。".into());
        }
    }
    let info = job.info();
    let mut screen = capture::ScreenCapture::new(job.region.clone())?;
    let mut encoder = RecordingEncoder::new(temporary, &info)?;
    let mut pending: Option<(Vec<u8>, i64)> = None;
    let mut frames = 0u64;
    let mut warning = None;
    let mut last_publish = Instant::now();
    let mut last_monitor_check = Instant::now();
    let interval = Duration::from_secs_f64(1. / info.frame_rate as f64);
    loop {
        let started = Instant::now();
        let mut control = job.lock();
        while control.paused && !control.finishing && !control.cancelled {
            control = job
                .changed
                .wait(control)
                .unwrap_or_else(|error| error.into_inner());
        }
        if control.cancelled {
            return Ok(None);
        }
        if control.finishing {
            break;
        }
        let generation = control.generation;
        drop(control);
        if last_monitor_check.elapsed() >= Duration::from_secs(1) {
            let valid = platform::monitors().is_ok_and(|monitors| {
                monitors.iter().any(|monitor| {
                    monitor.id == job.monitor.id
                        && monitor.x == job.monitor.x
                        && monitor.y == job.monitor.y
                        && monitor.width == job.monitor.width
                        && monitor.height == job.monitor.height
                })
            });
            if !valid {
                warning = Some("显示器配置已改变，已结束录屏并保存此前的画面。".into());
                break;
            }
            last_monitor_check = Instant::now();
        }
        let frame = screen.frame().and_then(|bgra| match info.format {
            RecordingFormat::Mp4 => video::bgra_to_nv12(bgra, job.region.width, job.region.height),
            RecordingFormat::Gif => video::bgra_to_rgb(
                bgra,
                job.region.width,
                job.region.height,
                info.width,
                info.height,
            ),
        });
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                warning = Some(error);
                break;
            }
        };
        let mut control = job.lock();
        if control.cancelled {
            return Ok(None);
        }
        if control.finishing {
            break;
        }
        if control.generation != generation {
            continue;
        }
        let now = job.origin.elapsed();
        if control.timeline.is_none() {
            control.timeline = Some(Timeline::new(now));
            control.info.starting = false;
            control.info.message = if info.format == RecordingFormat::Gif {
                format!(
                    "正在录制 GIF · {} FPS · 输出 {} × {} px",
                    info.frame_rate, info.width, info.height
                )
            } else if job.region.width != info.width || job.region.height != info.height {
                "正在录屏。为兼容 MP4，奇数宽高在边缘补齐 1 像素。".into()
            } else {
                "正在录屏 · MP4 · 无音频".into()
            };
        }
        let time = hns(control.timeline.as_ref().unwrap().elapsed(now));
        drop(control);
        if let Some((previous, timestamp)) = pending.take() {
            if let Err(error) = encoder.write(&previous, timestamp, (time - timestamp).max(1)) {
                warning = Some(error);
                break;
            }
            frames += 1;
        }
        pending = Some((frame, time));
        job.lock().info.frames = frames + 1;
        if last_publish.elapsed() >= Duration::from_millis(250) || frames == 0 {
            publish(app, job);
            last_publish = Instant::now();
        }
        if let Some(wait) = interval.checked_sub(started.elapsed()) {
            let control = job.lock();
            let _ = job.changed.wait_timeout_while(control, wait, |control| {
                !control.finishing
                    && !control.cancelled
                    && !control.paused
                    && control.generation == generation
            });
        }
    }
    let elapsed = {
        let mut control = job.lock();
        if let Some(clock) = control.timeline.as_mut() {
            clock.stop(job.origin.elapsed());
        }
        control.finishing = true;
        control.info.finishing = true;
        control.info.message = format!("正在完成 {} 文件，请稍候…", info.format.label());
        control
            .timeline
            .as_ref()
            .map(|clock| clock.elapsed(job.origin.elapsed()))
            .unwrap_or_default()
    };
    publish(app, job);
    if let Some((frame, timestamp)) = pending {
        encoder.write(&frame, timestamp, (hns(elapsed) - timestamp).max(1))?;
        frames += 1;
    }
    if frames == 0 {
        if job.lock().exit_after_finish {
            return Ok(None);
        }
        return Err("录屏尚未采集到画面，请重新开始。".into());
    }
    encoder.finish()?;
    if job.lock().cancelled {
        return Ok(None);
    }
    Ok(Some(RecordingResult {
        id: info.id,
        width: info.width,
        height: info.height,
        elapsed_ms: elapsed.as_millis() as u64,
        bytes: fs::metadata(temporary)
            .map_err(|error| format!("无法读取录屏文件：{error}"))?
            .len(),
        path: job.destination.to_string_lossy().into_owned(),
        warning,
        format: info.format,
    }))
}

fn run(app: AppHandle, job: Arc<Job>, temporary: TempPath) {
    let outcome = record(&job, &temporary, &app).and_then(|result| {
        let Some(result) = result else {
            return Ok(None);
        };
        temporary.persist(&job.destination).map_err(|error| {
            let message = error.error.to_string();
            match error.path.keep() {
                Ok(path) => format!(
                    "无法保存到目标位置：{message}。录制已保留在：{}",
                    path.display()
                ),
                Err(_) => format!("无法保存录屏文件：{message}"),
            }
        })?;
        Ok(Some(result))
    });
    match &outcome {
        Ok(Some(result)) => {
            diagnostics::write(
                "INFO",
                &format!(
                    "Recording saved: {} frames; {} bytes",
                    job.info().frames,
                    result.bytes
                ),
            );
            if let Some(warning) = &result.warning {
                diagnostics::write("WARN", warning);
            }
        }
        Ok(None) => diagnostics::write("INFO", "Recording cancelled"),
        Err(error) => diagnostics::write("ERROR", error),
    }
    let scheduled = app.clone();
    let _ = app.run_on_main_thread(move || {
        {
            let state = scheduled.state::<AppState>();
            let mut data = state.lock();
            if !data
                .recording
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &job))
            {
                return;
            }
            data.recording = None;
            data.capturing = false;
            if let Ok(Some(result)) = &outcome {
                data.recorded_video = Some(result.clone());
            }
        }
        if let Ok(panel) = window(&scheduled, "record") {
            let _ = panel.hide();
        }
        if let Ok(frame) = window(&scheduled, "record-region") {
            let _ = frame.hide();
        }
        scrolling::restore_tooltip(&scheduled);
        let _ = scheduled.emit_to("record", "recording-ended", ());
        let _ = scheduled.emit_to("record-region", "recording-ended", ());
        if job.lock().exit_after_finish && outcome.is_ok() {
            let _ = scheduled.global_shortcut().unregister_all();
            scheduled.exit(0);
            return;
        }
        match outcome {
            Ok(Some(result)) => {
                let _ = scheduled.emit_to("main", "recording-complete", &result);
                show_main(&scheduled);
            }
            Ok(None) => {
                if !job.background {
                    show_main(&scheduled);
                }
            }
            Err(error) => report_error(&scheduled, error),
        }
    });
}

#[tauri::command]
pub(super) fn recording_state(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<RecordingInfo>, String> {
    if !matches!(window.label(), "record" | "record-region") {
        return Err("当前窗口无法读取录屏状态。".into());
    }
    Ok(job(&app).ok().map(|job| job.info()))
}

#[tauri::command]
pub(super) fn pause_recording(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    paused: bool,
) -> Result<(), String> {
    require(&window, "record")?;
    let job = job(&app)?;
    {
        let mut control = job.lock();
        if control.info.id != id || control.finishing || control.info.starting {
            return Err("录屏正在准备或结束，请稍候。".into());
        }
        if let Some(clock) = control.timeline.as_mut() {
            if paused {
                clock.pause(job.origin.elapsed());
            } else {
                clock.resume(job.origin.elapsed());
            }
        }
        control.paused = paused;
        control.generation += 1;
        control.info.paused = paused;
        control.info.message = if paused {
            "已暂停，暂停时间不会计入视频。"
        } else {
            match control.info.format {
                RecordingFormat::Mp4 => "正在录屏 · MP4 · 无音频",
                RecordingFormat::Gif => "正在录制 GIF",
            }
        }
        .into();
    }
    job.changed.notify_all();
    publish(&app, &job);
    Ok(())
}

pub(super) fn finish(app: &AppHandle) -> Result<(), String> {
    let Ok(job) = job(app) else {
        return Ok(());
    };
    {
        let mut control = job.lock();
        if control.finishing {
            return Ok(());
        }
        if let Some(clock) = control.timeline.as_mut() {
            clock.stop(job.origin.elapsed());
        }
        control.finishing = true;
        control.info.finishing = true;
        control.info.message = format!("正在结束录制并保存 {}…", control.info.format.label());
    }
    job.changed.notify_all();
    publish(app, &job);
    Ok(())
}

#[tauri::command]
pub(super) fn finish_recording(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), String> {
    require(&window, "record")?;
    if job(&app)?.info().id != id {
        return Err("本次录屏已结束。".into());
    }
    finish(&app)
}

pub(super) fn request_exit(app: &AppHandle) -> Result<bool, String> {
    let Ok(job) = job(app) else {
        return Ok(false);
    };
    job.lock().exit_after_finish = true;
    finish(app)?;
    Ok(true)
}

#[tauri::command]
pub(super) fn cancel_recording(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
) -> Result<(), String> {
    require(&window, "record")?;
    let job = job(&app)?;
    {
        let mut control = job.lock();
        if control.info.id != id || control.finishing {
            return Err("本次录屏已结束或正在保存。".into());
        }
        control.cancelled = true;
        control.info.finishing = true;
        control.info.message = "正在取消录屏…".into();
    }
    job.changed.notify_all();
    publish(&app, &job);
    Ok(())
}

#[tauri::command]
pub(super) fn open_recording(
    app: AppHandle,
    window: WebviewWindow,
    id: u64,
    folder: bool,
) -> Result<(), String> {
    require(&window, "main")?;
    let path = app
        .state::<AppState>()
        .lock()
        .recorded_video
        .as_ref()
        .filter(|result| result.id == id)
        .map(|result| PathBuf::from(&result.path))
        .ok_or("没有可打开的录屏文件。")?;
    let path = if folder {
        path.parent().ok_or("录屏目录不可用。")?
    } else {
        &path
    };
    if !path.exists() {
        return Err("录屏文件或目录已被移动或删除。".into());
    }
    // Only open the saved recording (or its parent) in response to a user action.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("无法打开录屏，请从保存目录手动打开。".into());
    }
    Ok(())
}
