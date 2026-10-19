import { backend, imageUrl, type Session } from "./bridge";
import { Selection, bounds, clampPoint, type Point } from "./selection";

export async function mountOverlay(root: HTMLElement) {
  root.innerHTML = `<canvas id="selection-canvas" aria-label="截图选区画布"></canvas><p id="hint" class="overlay-hint" role="status"></p><div id="selection-toolbar" class="selection-toolbar" hidden><span id="selection-size"></span><button id="reset">重选&nbsp; R</button><button id="cancel">取消&nbsp; Esc</button><button id="accept" class="primary">确认&nbsp; Enter</button></div>`;
  const canvas = root.querySelector<HTMLCanvasElement>("canvas")!;
  const ctx = canvas.getContext("2d", { alpha: false })!;
  const hint = root.querySelector<HTMLParagraphElement>("#hint")!;
  const toolbar = root.querySelector<HTMLDivElement>("#selection-toolbar")!;
  const accept = root.querySelector<HTMLButtonElement>("#accept")!;
  const selection = new Selection();
  let image: HTMLImageElement | null = null;
  let session: Session | null = null;
  let cursor: Point = [0, 0];
  let hudVisible = true;
  let resolving = false;
  let raf = 0;
  let message = "";

  function draw() {
    raf = 0;
    const width = innerWidth,
      height = innerHeight,
      dpr = devicePixelRatio;
    const pw = Math.round(width * dpr),
      ph = Math.round(height * dpr);
    if (canvas.width !== pw || canvas.height !== ph) {
      canvas.width = pw;
      canvas.height = ph;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (image) ctx.drawImage(image, 0, 0, width, height);
    const path = new Path2D();
    selection.points.forEach(([x, y], i) =>
      i ? path.lineTo(x, y) : path.moveTo(x, y),
    );
    if (selection.complete) path.closePath();
    // Clip the shade rather than clearing the canvas: the frozen pixels stay intact.
    ctx.save();
    if (selection.complete) {
      const shade = new Path2D();
      shade.rect(0, 0, width, height);
      shade.addPath(path);
      ctx.clip(shade, "evenodd");
    }
    ctx.fillStyle = "rgba(8,22,29,.49)";
    ctx.fillRect(0, 0, width, height);
    ctx.restore();
    if (selection.points.length) {
      ctx.lineWidth = 3;
      ctx.strokeStyle = "#172b35";
      ctx.stroke(path);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = "#73f1ce";
      ctx.stroke(path);
      if (selection.mode === "polygon") {
        if (!selection.complete) {
          ctx.beginPath();
          ctx.moveTo(...selection.points.at(-1)!);
          ctx.lineTo(...cursor);
          ctx.setLineDash([5, 5]);
          ctx.lineWidth = 1;
          ctx.strokeStyle = "white";
          ctx.stroke();
          ctx.setLineDash([]);
        }
        for (const [x, y] of selection.points) {
          ctx.beginPath();
          ctx.arc(x, y, 4.5, 0, Math.PI * 2);
          ctx.fillStyle = "white";
          ctx.fill();
          ctx.strokeStyle = "#087f73";
          ctx.lineWidth = 1.5;
          ctx.stroke();
        }
      }
    }
    hint.textContent =
      message ||
      (selection.complete
        ? `拖动选区移动位置${selection.mode === "polygon" ? " · 拖动顶点调整轮廓" : ""}  |  Enter 确认 · R 重选 · Tab 隐藏提示 · Esc 取消`
        : selection.mode === "lasso"
          ? "自由套索：按住左键绘制，松开闭合  |  P 多边形 · Tab 隐藏提示 · Esc 取消"
          : "多边形：单击添加顶点，点击起点 / 双击 / Enter 闭合\n右键 / Backspace / Ctrl+Z 撤销 · L 套索 · Tab 隐藏提示 · Esc 取消");
    hint.hidden = !hudVisible;
    toolbar.hidden = !selection.complete || !hudVisible;
    const b = bounds(selection.points);
    root.querySelector("#selection-size")!.textContent =
      `约 ${Math.round((b.width * (session?.width ?? width)) / width)} × ${Math.round((b.height * (session?.height ?? height)) / height)} px`;
  }
  function render() {
    if (!raf) raf = requestAnimationFrame(draw);
  }
  function reset() {
    selection.reset();
    message = "";
    render();
  }
  function point(event: PointerEvent): Point {
    return clampPoint([event.clientX, event.clientY], innerWidth, innerHeight);
  }
  async function cancel() {
    if (!session || resolving) return;
    resolving = true;
    try {
      await backend.cancel(session.id);
      session = null;
      image = null;
      reset();
    } catch (error) {
      message = String(error);
      render();
    } finally {
      resolving = false;
    }
  }
  async function confirm() {
    if (!session || resolving) return;
    if (!selection.complete) {
      if (!selection.finish())
        message = "选区需要至少三个不共线的点。请继续绘制，或按 R 重选。";
      render();
      return;
    }
    resolving = true;
    accept.disabled = true;
    try {
      await backend.accept(
        session.id,
        selection.points,
        innerWidth,
        innerHeight,
      );
      session = null;
      image = null;
      reset();
    } catch (error) {
      message = String(error);
      render();
    } finally {
      resolving = false;
      accept.disabled = false;
    }
  }
  canvas.onpointerdown = (event) => {
    if (!session || resolving) return;
    if (event.button === 2) {
      selection.undo();
      message = "";
      render();
      return;
    }
    if (event.button !== 0) return;
    canvas.setPointerCapture(event.pointerId);
    cursor = point(event);
    message = "";
    selection.down(cursor);
    render();
  };
  canvas.onpointermove = (event) => {
    if (!session || resolving) return;
    cursor = point(event);
    const events = event.getCoalescedEvents?.() ?? [event];
    for (const sample of events.length ? events : [event])
      selection.move(point(sample), innerWidth, innerHeight);
    render();
  };
  canvas.onpointerup = (event) => {
    if (!session || resolving || event.button !== 0) return;
    selection.up(point(event), innerWidth, innerHeight);
    if (canvas.hasPointerCapture(event.pointerId))
      canvas.releasePointerCapture(event.pointerId);
    if (selection.mode === "lasso" && !selection.complete)
      message = "选区需要至少三个不共线的点。请按 R 重新绘制。";
    render();
  };
  canvas.onpointercancel = () => reset();
  canvas.ondblclick = () => {
    if (!resolving && selection.mode === "polygon" && !selection.complete) {
      selection.finish();
      render();
    }
  };
  root.oncontextmenu = (event) => event.preventDefault();
  root.querySelector<HTMLButtonElement>("#reset")!.onclick = reset;
  root.querySelector<HTMLButtonElement>("#cancel")!.onclick = () =>
    void cancel();
  accept.onclick = () => void confirm();
  window.addEventListener("keydown", (event) => {
    if (resolving) return;
    if (
      ["Escape", "Enter", "Backspace", "Delete", "Tab"].includes(event.key) ||
      ["KeyL", "KeyP", "KeyR", "KeyZ"].includes(event.code)
    )
      event.preventDefault();
    message = "";
    if (event.key === "Escape") void cancel();
    else if (event.key === "Enter") void confirm();
    else if (
      event.key === "Backspace" ||
      event.key === "Delete" ||
      (event.ctrlKey && event.code === "KeyZ")
    )
      selection.undo();
    else if (event.code === "KeyR") reset();
    else if (event.code === "KeyL" || event.code === "KeyP") {
      selection.reset(event.code === "KeyL" ? "lasso" : "polygon");
      void backend.mode(selection.mode).catch((error: unknown) => {
        message = String(error);
        render();
      });
    } else if (event.key === "Tab") hudVisible = !hudVisible;
    render();
  });
  window.addEventListener("resize", render);
  async function load(next: Session) {
    session = next;
    image = null;
    resolving = false;
    hudVisible = true;
    message = "";
    selection.reset(next.mode);
    const frozen = new Image();
    frozen.src = imageUrl(next.id, "source");
    try {
      await frozen.decode();
      if (session?.id !== next.id) return;
      image = frozen;
      // Paint before asking Rust to expose the window, avoiding a blank overlay.
      draw();
      await backend.showOverlay(next.id);
      render();
    } catch (error) {
      if (session?.id === next.id) {
        await backend.cancel(next.id);
        session = null;
        throw error;
      }
    }
  }
  await backend.on<Session>(
    "capture-started",
    (next) =>
      void load(next).catch((error: unknown) => {
        message = String(error);
        render();
      }),
  );
  const initial = await backend.session();
  if (initial) await load(initial);
}
