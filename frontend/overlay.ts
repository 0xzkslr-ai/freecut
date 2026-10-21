import { backend, imageUrl, type Session } from "./bridge";
import { Selection, bounds, clampPoint, type Point } from "./selection";
import { imageViewport, viewportPoints } from "./viewport";

export async function mountOverlay(root: HTMLElement) {
  root.innerHTML = `<canvas id="selection-canvas" aria-label="截图选区画布"></canvas><p id="hint" class="overlay-hint" role="status"></p><div id="selection-toolbar" class="selection-toolbar" hidden><span id="selection-size"></span><button id="reset">重选&nbsp; R</button><button id="cancel">取消&nbsp; Esc</button><button id="scroll-capture" hidden>滚动截图</button><button id="record-capture" hidden>录屏</button><button id="accept" class="primary">确认&nbsp; Enter</button></div>`;
  const canvas = root.querySelector<HTMLCanvasElement>("canvas")!;
  const ctx = canvas.getContext("2d", { alpha: false })!;
  const hint = root.querySelector<HTMLParagraphElement>("#hint")!;
  const toolbar = root.querySelector<HTMLDivElement>("#selection-toolbar")!;
  const accept = root.querySelector<HTMLButtonElement>("#accept")!;
  const scroll = root.querySelector<HTMLButtonElement>("#scroll-capture")!;
  const record = root.querySelector<HTMLButtonElement>("#record-capture")!;
  const selection = new Selection();
  let image: HTMLImageElement | null = null;
  let session: Session | null = null;
  let cursor: Point = [0, 0];
  let hudVisible = true;
  let resolving = false;
  let raf = 0;
  let message = "";
  const viewport = () =>
    imageViewport(
      innerWidth,
      innerHeight,
      session?.width ?? innerWidth,
      session?.height ?? innerHeight,
      !!session?.crop,
    );

  function draw() {
    raf = 0;
    const view = viewport();
    const width = view.width,
      height = view.height,
      dpr = devicePixelRatio;
    const pw = Math.round(innerWidth * dpr),
      ph = Math.round(innerHeight * dpr);
    if (canvas.width !== pw || canvas.height !== ph) {
      canvas.width = pw;
      canvas.height = ph;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#172b35";
    ctx.fillRect(0, 0, innerWidth, innerHeight);
    ctx.translate(view.x, view.y);
    ctx.fillStyle = "#e3e9ec";
    ctx.fillRect(0, 0, width, height);
    if (image) ctx.drawImage(image, 0, 0, width, height);
    const path = new Path2D();
    selection.points.forEach(([x, y], i) =>
      i ? path.lineTo(x, y) : path.moveTo(x, y),
    );
    if (selection.complete || selection.mode === "rectangle") path.closePath();
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
      if (selection.mode !== "lasso") {
        if (selection.mode === "polygon" && !selection.complete) {
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
      (session?.recording
        ? selection.complete
          ? "拖动选区或角点调整范围 · Enter 开始录屏 · Esc 取消\n选择保存位置后开始录制，暂停时间不计入视频。"
          : "录屏：拖出一个矩形区域 · Esc 取消\nMP4 · 画面与鼠标指针 · 无音频"
        : selection.complete
          ? `拖动选区移动位置${selection.mode === "polygon" ? " · 拖动顶点调整轮廓" : selection.mode === "rectangle" ? " · 拖动角点调整大小" : ""}  |  Enter 确认 · R 重选 · Tab 隐藏提示 · Esc 取消${selection.mode === "rectangle" && !session?.crop ? "\n滚动截图：手动移动地图，按全局截图键结束；控制条无空位时收起到托盘。" : ""}`
          : selection.mode === "lasso"
            ? "自由套索：按住左键绘制，松开闭合  |  P 多边形 · M 矩形 · Tab 隐藏提示 · Esc 取消"
            : selection.mode === "rectangle"
              ? "矩形选取：按住左键拖动，松开完成  |  L 套索 · P 多边形 · Tab 隐藏提示 · Esc 取消"
              : "多边形：单击添加顶点，点击起点 / 双击 / Enter 闭合\n右键 / Backspace / Ctrl+Z 撤销 · L 套索 · M 矩形 · Tab 隐藏提示 · Esc 取消");
    hint.hidden = !hudVisible;
    toolbar.hidden = !selection.complete || !hudVisible;
    scroll.hidden =
      selection.mode !== "rectangle" || !!session?.crop || !!session?.recording;
    record.hidden =
      selection.mode !== "rectangle" || !!session?.crop || !!session?.recording;
    accept.textContent = session?.recording
      ? "开始录屏 · Enter"
      : "确认 · Enter";
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
    const view = viewport();
    return clampPoint(
      [event.clientX - view.x, event.clientY - view.y],
      view.width,
      view.height,
    );
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
  async function confirm(action: "image" | "scroll" | "record" = "image") {
    if (!session || resolving) return;
    if (!selection.complete) {
      if (!selection.finish())
        message =
          selection.mode === "rectangle"
            ? "请拖出有宽度和高度的矩形，或按 R 重选。"
            : "选区需要至少三个不共线的点。请继续绘制，或按 R 重选。";
      render();
      return;
    }
    resolving = true;
    accept.disabled = scroll.disabled = record.disabled = true;
    try {
      const expectedId = session.id;
      const view = viewport();
      const recording = action === "record" || !!session.recording;
      const accepted = await (
        recording
          ? backend.record
          : action === "scroll"
            ? backend.scroll
            : backend.accept
      )(
        session.id,
        viewportPoints(
          selection.points,
          view.width,
          view.height,
          innerWidth,
          innerHeight,
        ),
        innerWidth,
        innerHeight,
      );
      if (session?.id !== expectedId || (recording && accepted === false))
        return;
      session = null;
      image = null;
      reset();
    } catch (error) {
      message = String(error);
      render();
    } finally {
      resolving = false;
      accept.disabled = scroll.disabled = record.disabled = false;
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
    const view = viewport();
    if (
      event.clientX < view.x - 0.5 ||
      event.clientY < view.y - 0.5 ||
      event.clientX > view.x + view.width + 0.5 ||
      event.clientY > view.y + view.height + 0.5
    )
      return;
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
      selection.move(point(sample), viewport().width, viewport().height);
    render();
  };
  canvas.onpointerup = (event) => {
    if (!session || resolving || event.button !== 0) return;
    selection.up(point(event), viewport().width, viewport().height);
    if (canvas.hasPointerCapture(event.pointerId))
      canvas.releasePointerCapture(event.pointerId);
    if (selection.mode !== "polygon" && !selection.complete)
      message =
        selection.mode === "rectangle"
          ? "请拖出有宽度和高度的矩形，或按 R 重选。"
          : "选区需要至少三个不共线的点。请按 R 重新绘制。";
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
  scroll.onclick = () => void confirm("scroll");
  record.onclick = () => void confirm("record");
  window.addEventListener("keydown", (event) => {
    if (resolving) return;
    if (
      ["Escape", "Enter", "Backspace", "Delete", "Tab"].includes(event.key) ||
      ["KeyL", "KeyP", "KeyM", "KeyR", "KeyZ"].includes(event.code)
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
    else if (["KeyL", "KeyP", "KeyM"].includes(event.code)) {
      if (session?.recording) return;
      selection.reset(
        event.code === "KeyL"
          ? "lasso"
          : event.code === "KeyP"
            ? "polygon"
            : "rectangle",
      );
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
