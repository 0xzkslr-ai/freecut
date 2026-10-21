import { backend, type RecordingInfo } from "./bridge";

export function recordingTime(milliseconds: number): string {
  const seconds = Number.isFinite(milliseconds)
    ? Math.floor(Math.max(0, milliseconds) / 1000)
    : 0;
  return `${Math.floor(seconds / 3600)
    .toString()
    .padStart(2, "0")}:${Math.floor((seconds % 3600) / 60)
    .toString()
    .padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}`;
}

export function recordingStatus(
  info: Pick<RecordingInfo, "starting" | "paused" | "finishing">,
) {
  if (info.finishing) return { phase: "finishing", label: "正在保存" };
  if (info.starting) return { phase: "starting", label: "准备录制" };
  if (info.paused) return { phase: "paused", label: "已暂停" };
  return { phase: "recording", label: "录制中" };
}

export async function mountRecord(root: HTMLElement) {
  root.dataset.phase = "starting";
  root.innerHTML = `<main class="record-panel">
    <header class="record-heading" data-tauri-drag-region="deep" title="拖动标题移动控制窗">
      <div class="record-brand"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 7V3h4M13 3h4v4M17 13v4h-4M7 17H3v-4"/><circle cx="10" cy="10" r="2.5"/></svg><span>FreeCut <span class="record-title">区域录屏</span></span></div>
      <span class="record-status"><i class="record-dot" aria-hidden="true"></i><span id="record-state">准备录制</span></span>
    </header>
    <div class="record-timing"><p id="record-time" class="record-time" aria-label="已录制时长">00:00:00</p><div class="record-format"><strong id="record-output-format">MP4</strong><span id="record-output-detail">无音频</span><span id="record-output-size"></span></div></div>
    <button id="record-locate" class="record-region-button" title="定位录制范围，边框会加粗高亮" disabled>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 3H3v5M16 3h5v5M21 16v5h-5M8 21H3v-5"/><rect x="7" y="7" width="10" height="10" rx="1"/></svg>
      <span class="record-region-description"><strong id="record-size">正在准备录制区域</strong><span id="record-position">录制范围将以边框标出</span></span><span class="record-locate-label">定位范围</span>
    </button>
    <p id="record-message" role="status">正在准备录屏…</p>
    <div class="record-actions">
      <button id="record-cancel" class="record-discard" aria-label="取消录制" title="取消本次录制，不保存"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5M14 11v5"/></svg></button>
      <button id="record-pause" disabled><svg viewBox="0 0 24 24" aria-hidden="true"><g class="record-pause-glyph"><path d="M8 5v14M16 5v14"/></g><g class="record-resume-glyph"><path d="m8 5 11 7-11 7Z"/></g></svg><span id="record-pause-label">暂停</span></button>
      <button id="record-finish" class="record-stop" disabled><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2"/></svg><span>结束并保存</span></button>
    </div>
    <p id="record-hotkey"></p>
  </main>`;
  const get = <T extends HTMLElement>(id: string) =>
    root.querySelector<T>(`#${id}`)!;
  const pause = get<HTMLButtonElement>("record-pause");
  const finish = get<HTMLButtonElement>("record-finish");
  const cancel = get<HTMLButtonElement>("record-cancel");
  const locate = get<HTMLButtonElement>("record-locate");
  let current: RecordingInfo | null = null;
  let busy = false;
  function buttons() {
    pause.disabled = busy || !current || current.starting || current.finishing;
    finish.disabled = busy || !current || current.finishing;
    cancel.disabled = busy || !current || current.finishing;
    locate.disabled = busy || !current || current.starting || current.finishing;
  }
  function update(info: RecordingInfo) {
    current = info;
    get("record-output-format").textContent =
      info.format === "gif" ? "GIF" : "MP4";
    get("record-output-detail").textContent =
      `${info.frameRate ?? 30} FPS · ${info.format === "gif" ? "循环播放" : "无音频"}`;
    get("record-output-size").textContent =
      `输出 ${info.width} × ${info.height}`;
    get("record-pause-label").textContent = info.paused ? "继续" : "暂停";
    const status = recordingStatus(info);
    root.dataset.phase = status.phase;
    get("record-time").textContent = recordingTime(info.elapsedMs);
    get("record-state").textContent = status.label;
    const region = info.region ?? {
      x: 0,
      y: 0,
      width: info.width,
      height: info.height,
    };
    get("record-size").textContent = `${region.width} × ${region.height} px`;
    get("record-position").textContent =
      `${info.monitorName ?? "当前屏幕"} · 左上角 (${region.x}, ${region.y})`;
    get("record-message").textContent = info.message;
    get("record-hotkey").textContent =
      `${info.hotkey} 结束录屏 · 空格 暂停 / 继续`;
    root.classList.toggle(
      "record-paused",
      info.paused || info.starting || info.finishing,
    );
    buttons();
  }
  async function attempt(action: (info: RecordingInfo) => Promise<void>) {
    if (!current || busy) return;
    busy = true;
    buttons();
    try {
      await action(current);
    } catch (error) {
      get("record-message").textContent = String(error);
    } finally {
      busy = false;
      buttons();
    }
  }
  pause.onclick = () =>
    void attempt((info) => backend.pauseRecording(info.id, !info.paused));
  finish.onclick = () =>
    void attempt((info) => backend.finishRecording(info.id));
  cancel.onclick = () =>
    void attempt((info) => backend.cancelRecording(info.id));
  locate.onclick = () =>
    void attempt((info) => backend.highlightRecordingRegion(info.id));
  window.addEventListener("keydown", (event) => {
    if (event.target instanceof HTMLElement && event.target.closest("button"))
      return;
    if (event.key === "Enter") {
      event.preventDefault();
      finish.click();
    } else if (event.code === "Space") {
      event.preventDefault();
      pause.click();
    }
  });
  await backend.on<RecordingInfo>("recording-progress", update);
  await backend.on("recording-ended", () => {
    current = null;
    buttons();
  });
  const initial = await backend.recordingState();
  if (initial && !current) update(initial);
}
