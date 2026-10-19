import { backend, type ScrollInfo } from "./bridge";

export async function mountScroll(root: HTMLElement) {
  root.innerHTML = `<main class="scroll-panel"><div class="scroll-heading"><strong>滚动截图 <span id="scroll-state"></span></strong><span id="scroll-size"></span></div><canvas id="coverage-map" role="img" aria-label="地图采集覆盖范围"></canvas><p id="coverage-legend" class="muted small">绿色：已采集 · 线框：当前位置 · 棋盘格：尚未覆盖</p><p id="scroll-message" role="status">等待开始采集…</p><p id="scroll-hotkey" class="muted small"></p><div class="actions"><button id="scroll-cancel">取消</button><button id="scroll-pause">暂停</button><button id="scroll-finish" class="primary">结束并预览</button></div></main>`;
  const get = <T extends HTMLElement>(id: string) =>
    root.querySelector<T>(`#${id}`)!;
  const pause = get<HTMLButtonElement>("scroll-pause");
  const finish = get<HTMLButtonElement>("scroll-finish");
  const cancel = get<HTMLButtonElement>("scroll-cancel");
  let state: ScrollInfo | null = null;
  let busy = false;
  function buttons() {
    pause.disabled = finish.disabled = busy || !!state?.finishing;
    cancel.disabled = busy;
  }
  function drawCoverage(next: ScrollInfo) {
    const coverage = next.coverage;
    if (!coverage) return;
    const canvas = get<HTMLCanvasElement>("coverage-map");
    const width = canvas.clientWidth,
      height = canvas.clientHeight;
    if (width <= 0 || height <= 0) return;
    canvas.width = Math.round(width * devicePixelRatio);
    canvas.height = Math.round(height * devicePixelRatio);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
    for (let y = 0; y < height; y += 8)
      for (let x = 0; x < width; x += 8) {
        ctx.fillStyle = ((x + y) / 8) % 2 ? "#edf2f3" : "#dfe7e9";
        ctx.fillRect(x, y, 8, 8);
      }
    const b = coverage.bounds;
    const scale = Math.min((width - 12) / b.width, (height - 12) / b.height);
    const ox = (width - b.width * scale) / 2 - b.x * scale;
    const oy = (height - b.height * scale) / 2 - b.y * scale;
    ctx.fillStyle = "#70bfa2";
    for (const r of coverage.regions)
      ctx.fillRect(
        ox + r.x * scale,
        oy + r.y * scale,
        r.width * scale,
        r.height * scale,
      );
    const view = coverage.viewport;
    ctx.lineWidth = 2;
    ctx.strokeStyle = next.recovering ? "#9b5d12" : "#075b54";
    ctx.setLineDash(next.recovering ? [4, 3] : []);
    ctx.strokeRect(
      ox + view.x * scale,
      oy + view.y * scale,
      view.width * scale,
      view.height * scale,
    );
  }
  function update(next: ScrollInfo) {
    state = next;
    get("scroll-size").textContent =
      `${next.width} × ${next.height} px · ${next.frames} 帧`;
    get("scroll-message").textContent = next.message;
    get("scroll-hotkey").textContent =
      `${next.hotkey} 结束 · 托盘「显示主窗口」可暂停并打开控制条`;
    pause.textContent = next.paused ? "继续采集" : "暂停";
    root.classList.toggle("scroll-paused", next.paused);
    root.classList.toggle("scroll-recovering", next.recovering);
    get("coverage-legend").textContent = next.recovering
      ? "绿色：已采集 · 虚线：最后定位 · 棋盘格：尚未覆盖"
      : "绿色：已采集 · 线框：当前位置 · 棋盘格：尚未覆盖";
    get("scroll-state").textContent = next.finishing
      ? "合成中"
      : next.paused
        ? "已暂停"
        : next.recovering
          ? "定位中"
          : "采集中";
    buttons();
    drawCoverage(next);
  }
  async function attempt(action: (current: ScrollInfo) => Promise<void>) {
    if (!state || busy) return;
    busy = true;
    buttons();
    try {
      await action(state);
    } catch (error) {
      get("scroll-message").textContent = String(error);
    } finally {
      busy = false;
      buttons();
    }
  }
  pause.onclick = () =>
    void attempt((current) => backend.pauseScroll(current.id, !current.paused));
  finish.onclick = () =>
    void attempt((current) => backend.finishScroll(current.id));
  cancel.onclick = () =>
    void attempt((current) => backend.cancelScroll(current.id));
  window.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      finish.click();
    }
  });
  await backend.on<ScrollInfo>("scroll-progress", update);
  await backend.on("scroll-ended", () => {
    state = null;
  });
  window.addEventListener("resize", () => {
    if (state) drawCoverage(state);
  });
  const initial = await backend.scrollState();
  if (initial && !state) update(initial);
}
