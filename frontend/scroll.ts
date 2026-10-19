import { backend, type ScrollInfo } from "./bridge";

export async function mountScroll(root: HTMLElement) {
  root.innerHTML = `<main class="scroll-panel"><div class="scroll-heading"><strong>滚动截图</strong><span id="scroll-size"></span></div><p id="scroll-message" role="status">等待开始采集…</p><p id="scroll-hotkey" class="muted small"></p><div class="actions"><button id="scroll-cancel">取消</button><button id="scroll-pause">暂停</button><button id="scroll-finish" class="primary">结束并预览</button></div></main>`;
  const get = <T extends HTMLElement>(id: string) =>
    root.querySelector<T>(`#${id}`)!;
  const pause = get<HTMLButtonElement>("scroll-pause");
  const finish = get<HTMLButtonElement>("scroll-finish");
  const cancel = get<HTMLButtonElement>("scroll-cancel");
  let state: ScrollInfo | null = null;
  let busy = false;
  function update(next: ScrollInfo) {
    state = next;
    get("scroll-size").textContent =
      `${next.width} × ${next.height} px · ${next.frames} 帧`;
    get("scroll-message").textContent = next.message;
    get("scroll-hotkey").textContent =
      `${next.hotkey} 结束 · 托盘「显示主窗口」可暂停并打开控制条`;
    pause.textContent = next.paused ? "继续采集" : "暂停";
    root.classList.toggle("scroll-paused", next.paused);
  }
  async function attempt(action: (current: ScrollInfo) => Promise<void>) {
    if (!state || busy) return;
    busy = true;
    pause.disabled = finish.disabled = cancel.disabled = true;
    try {
      await action(state);
    } catch (error) {
      get("scroll-message").textContent = String(error);
    } finally {
      busy = false;
      pause.disabled = finish.disabled = cancel.disabled = false;
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
  const initial = await backend.scrollState();
  if (initial && !state) update(initial);
}
