import {
  backend,
  imageUrl,
  type Monitor,
  type Preview,
  type RecordingResult,
} from "./bridge";
import { recordingTime } from "./record";
import { hotkeyFromEvent } from "./hotkey";
import type { Mode } from "./selection";

export async function mountMain(root: HTMLElement) {
  root.innerHTML = `
    <main class="main-window">
      <header><h1>FreeCut</h1><p>让截图，自由一点。</p><button id="background">后台运行</button></header>
      <div class="workspace">
        <aside class="card sidebar">
          <h2>选择你的工具</h2>
          <div class="modes" role="group" aria-label="截图工具">
            <button class="mode" data-mode="lasso" aria-pressed="true"><strong>自由套索</strong><span>按住鼠标，画出任意轮廓</span></button>
            <button class="mode" data-mode="polygon" aria-pressed="false"><strong>多边形</strong><span>逐点连线，精确勾勒边界</span></button>
            <button class="mode" data-mode="rectangle" aria-pressed="false"><strong>矩形选取</strong><span>拖动框选，也可开始滚动拼接</span></button>
          </div>
          <label for="monitor">截取屏幕</label><select id="monitor" aria-label="选择要截取的显示器"></select>
          <button class="primary capture" id="capture">开始截图</button><p class="muted small">Ctrl + Shift + A · 窗口内快捷键</p>
          <button id="record">开始录屏</button><p class="muted small">框选区域 · MP4 · 无音频</p>
          <label for="hotkey">全局截图快捷键</label>
          <div class="hotkey-row"><input id="hotkey" readonly spellcheck="false" aria-describedby="hotkey-status" title="点击后按下新的组合键，再点击应用" /><button id="apply-hotkey">应用</button></div>
          <p id="hotkey-status" class="muted small" role="status"></p>
          <p class="muted sidebar-hint">Enter 确认 · Esc 取消</p>
        </aside>
        <section class="card preview-card" aria-label="截图预览">
          <div class="preview-heading"><h2>截图预览</h2><span id="dimensions" class="muted">尚未截图</span></div>
          <div class="checker" id="preview"><div class="empty"><svg viewBox="0 0 64 64" aria-hidden="true"><path d="M13 14 47 19 52 40 32 53 12 38Z"/><circle cx="13" cy="14" r="4"/></svg><strong>只截取你想要的形状</strong><p>选择左侧工具，开始一次截图。<br/>选区以外的部分会变为透明。</p></div><img id="preview-image" alt="透明截图预览" hidden draggable="false" /><div id="video-result" class="video-result" hidden><span class="video-symbol" aria-hidden="true">▶</span><strong>录屏已保存</strong><p id="video-details"></p><p id="video-path"></p></div></div>
          <p id="preview-note" class="muted small">棋盘格表示透明区域，不会出现在导出的图片中。</p>
          <div class="actions"><button id="open-video" hidden>打开视频</button><button id="open-video-folder" hidden>打开文件夹</button><button id="edit" disabled>再次裁剪</button><button id="copy" disabled>复制图片&nbsp; Ctrl+C</button><button id="save" class="primary" disabled>保存 PNG&nbsp; Ctrl+S</button></div>
        </section>
      </div>
      <p id="status" class="status" role="status">就绪 · 选择一个工具，开始截取屏幕。</p>
    </main>`;
  const get = <T extends HTMLElement>(id: string) =>
    root.querySelector<T>(`#${id}`)!;
  const monitor = get<HTMLSelectElement>("monitor");
  const hotkey = get<HTMLInputElement>("hotkey");
  const hotkeyStatus = get<HTMLParagraphElement>("hotkey-status");
  const capture = get<HTMLButtonElement>("capture");
  const record = get<HTMLButtonElement>("record");
  const copy = get<HTMLButtonElement>("copy");
  const save = get<HTMLButtonElement>("save");
  const edit = get<HTMLButtonElement>("edit");
  let busy = false;
  let hasImage = false;
  let refreshId = 0;
  let video: RecordingResult | null = null;
  const status = (message: string) => {
    get("status").textContent = message;
  };
  const attempt = async (action: () => Promise<unknown>) => {
    if (busy) return;
    busy = true;
    capture.disabled = true;
    record.disabled = true;
    try {
      await action();
    } catch (error) {
      status(String(error));
    } finally {
      busy = false;
      capture.disabled = !monitor.options.length;
      record.disabled = !monitor.options.length;
    }
  };
  const setModeButtons = (mode: Mode) => {
    root
      .querySelectorAll<HTMLButtonElement>(".mode")
      .forEach((button) =>
        button.setAttribute(
          "aria-pressed",
          String(button.dataset.mode === mode),
        ),
      );
  };
  const updatePreview = (preview: Preview) => {
    get("video-result").hidden = true;
    get("open-video").hidden = get("open-video-folder").hidden = true;
    edit.hidden = copy.hidden = save.hidden = false;
    root.querySelector(".preview-heading h2")!.textContent = "截图预览";
    get("preview-note").textContent =
      "棋盘格表示透明区域，不会出现在导出的图片中。";
    const img = get<HTMLImageElement>("preview-image");
    img.src = imageUrl(preview.id, "preview");
    img.hidden = false;
    root.querySelector<HTMLElement>(".empty")!.hidden = true;
    get("dimensions").textContent = `${preview.width} × ${preview.height} px`;
    hasImage = true;
    copy.disabled = save.disabled = edit.disabled = false;
    status("截图完成 · 可以再次裁剪、复制图片或保存透明 PNG。");
  };
  const updateRecording = (result: RecordingResult) => {
    video = result;
    hasImage = false;
    get("preview-image").hidden = true;
    root.querySelector<HTMLElement>(".empty")!.hidden = true;
    get("video-result").hidden = false;
    get("open-video").hidden = get("open-video-folder").hidden = false;
    edit.hidden = copy.hidden = save.hidden = true;
    root.querySelector(".preview-heading h2")!.textContent = "录屏完成";
    get("dimensions").textContent = `${result.width} × ${result.height} px`;
    get("video-details").textContent =
      `${recordingTime(result.elapsedMs)} · ${(result.bytes / 1e6).toFixed(2)} MB · MP4`;
    get("video-path").textContent = result.path;
    get("preview-note").textContent =
      "录屏已保存到所选位置，可使用系统播放器打开。";
    status(result.warning ? `录屏已保存 · ${result.warning}` : "录屏已保存。");
  };
  const updateMonitors = (monitors: Monitor[]) => {
    const previous = monitor.value;
    monitor.replaceChildren(
      ...monitors.map(
        (screen, i) =>
          new Option(
            `${i + 1}. ${screen.name} · ${screen.width} × ${screen.height}${screen.primary ? " · 主屏" : ""}`,
            screen.id,
          ),
      ),
    );
    if (monitors.some((m) => m.id === previous)) monitor.value = previous;
    else
      monitor.value =
        (monitors.find((m) => m.primary) ?? monitors[0])?.id ?? "";
    capture.disabled = busy || !monitors.length;
    record.disabled = busy || !monitors.length;
  };
  capture.onclick = () => void attempt(() => backend.capture(monitor.value));
  record.onclick = () =>
    void attempt(() => backend.recordSelection(monitor.value));
  get("open-video").onclick = () =>
    void attempt(async () => {
      if (video) await backend.openRecording(video.id, false);
    });
  get("open-video-folder").onclick = () =>
    void attempt(async () => {
      if (video) await backend.openRecording(video.id, true);
    });
  get("background").onclick = () => void attempt(() => backend.background());
  root.querySelectorAll<HTMLButtonElement>(".mode").forEach((button) => {
    button.onclick = () =>
      void attempt(async () => {
        const mode = button.dataset.mode as Mode;
        await backend.mode(mode);
        setModeButtons(mode);
      });
  });
  hotkey.onkeydown = (event) => {
    if (event.key === "Tab") return;
    event.preventDefault();
    event.stopPropagation();
    const value = hotkeyFromEvent(event);
    if (value) {
      hotkey.value = value;
      hotkeyStatus.textContent = "点击「应用」使新快捷键生效。";
    } else
      hotkeyStatus.textContent =
        "使用 Ctrl / Alt 和字母、数字或 F1–F24（F12 除外），可搭配 Shift。";
  };
  get("apply-hotkey").onclick = () =>
    void attempt(async () => {
      try {
        hotkey.value = await backend.hotkey(hotkey.value);
        hotkeyStatus.textContent = `${hotkey.value} · 在其他应用中也能截图`;
      } catch (error) {
        hotkeyStatus.textContent = String(error);
      }
    });
  const doCopy = () =>
    attempt(async () => {
      if (hasImage) {
        await backend.copy();
        status("图片已复制。");
      }
    });
  const doSave = () =>
    attempt(async () => {
      if (hasImage) {
        const path = await backend.save();
        if (path) status(`已保存：${path}`);
      }
    });
  copy.onclick = () => void doCopy();
  save.onclick = () => void doSave();
  edit.onclick = () => void attempt(() => backend.edit());
  window.addEventListener("keydown", (event) => {
    if (
      event.target === hotkey ||
      !event.ctrlKey ||
      event.altKey ||
      event.metaKey
    )
      return;
    const action =
      event.code === "KeyA" && event.shiftKey
        ? () => attempt(() => backend.capture(monitor.value))
        : event.code === "KeyS"
          ? doSave
          : event.code === "KeyC" && !window.getSelection()?.toString()
            ? doCopy
            : event.code === "KeyQ"
              ? () => backend.quit()
              : null;
    if (action) {
      event.preventDefault();
      void action();
    }
  });
  window.addEventListener("focus", () => {
    const request = ++refreshId;
    void backend
      .monitors()
      .then((monitors) => {
        if (request === refreshId) updateMonitors(monitors);
      })
      .catch((error: unknown) => status(String(error)));
  });
  await backend.on<Preview>("preview-changed", updatePreview);
  await backend.on<RecordingResult>("recording-complete", updateRecording);
  await backend.on<string>("capture-error", status);
  await backend.on<Mode>("mode-changed", setModeButtons);
  await backend.on<string>("hotkey-status", (message) => {
    hotkeyStatus.textContent = message;
  });
  const initial = await backend.init();
  updateMonitors(initial.monitors);
  setModeButtons(initial.settings.mode);
  hotkey.value = initial.settings.hotkey;
  hotkeyStatus.textContent =
    initial.hotkeyError ?? `${initial.settings.hotkey} · 在其他应用中也能截图`;
  get<HTMLButtonElement>("background").disabled = !initial.trayAvailable;
  if (initial.preview) updatePreview(initial.preview);
  if (
    initial.recordedVideo &&
    (!initial.preview || initial.recordedVideo.id > initial.preview.id)
  )
    updateRecording(initial.recordedVideo);
  await backend.ready();
}
