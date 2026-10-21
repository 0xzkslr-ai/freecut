import { backend, type RecordingInfo } from "./bridge";
import { recordingStatus } from "./record";

export async function mountRecordingRegion(root: HTMLElement) {
  root.dataset.phase = "starting";
  root.innerHTML = `<div class="recording-region" role="img" aria-label="录制范围">
    <i class="region-corner region-corner-tl"></i><i class="region-corner region-corner-tr"></i><i class="region-corner region-corner-bl"></i><i class="region-corner region-corner-br"></i>
    <div class="region-label"><i aria-hidden="true"></i><strong id="region-state">录制范围</strong><span id="region-size"></span></div>
  </div>`;
  const frame = root.querySelector<HTMLElement>(".recording-region")!;
  let currentId: number | null = null;
  let current: RecordingInfo | null = null;
  let prepared = false;
  let acknowledgedId: number | null = null;
  let pendingId: number | null = null;
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  function layout() {
    root.classList.toggle(
      "region-small",
      innerWidth < 240 || innerHeight < 100,
    );
  }
  function update(info: RecordingInfo) {
    if (currentId !== info.id) root.classList.remove("region-highlight");
    currentId = info.id;
    current = info;
    const state = recordingStatus(info);
    root.dataset.phase = state.phase;
    const region = info.region ?? info;
    root.querySelector("#region-state")!.textContent = state.label;
    root.querySelector("#region-size")!.textContent =
      `${region.width} × ${region.height} px`;
    frame.setAttribute(
      "aria-label",
      `${state.label}，录制范围 ${region.width} × ${region.height} 像素`,
    );
    acknowledge(info);
  }
  function acknowledge(info: RecordingInfo) {
    if (
      !prepared ||
      !info.starting ||
      info.finishing ||
      acknowledgedId === info.id ||
      pendingId === info.id
    )
      return;
    pendingId = info.id;
    // Native code has now shown the window. Hidden WebViews may suspend RAF,
    // so paint acknowledgements are only requested after the ready/show handshake.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (currentId !== info.id) return;
        void backend
          .recordingRegionShown(info.id)
          .then(() => {
            acknowledgedId = info.id;
            pendingId = null;
          })
          .catch(() => {
            pendingId = null;
          });
      }),
    );
  }
  await backend.on<RecordingInfo>("recording-progress", update);
  await backend.on<number>("recording-highlight", (id) => {
    if (currentId !== null && id !== currentId) return;
    clearTimeout(highlightTimer);
    root.classList.add("region-highlight");
    highlightTimer = setTimeout(
      () => root.classList.remove("region-highlight"),
      1500,
    );
  });
  await backend.on("recording-ended", () => {
    currentId = null;
    current = null;
    pendingId = acknowledgedId = null;
    clearTimeout(highlightTimer);
    root.classList.remove("region-highlight");
    root.dataset.phase = "starting";
    root.querySelector("#region-state")!.textContent = "录制范围";
    root.querySelector("#region-size")!.textContent = "";
  });
  window.addEventListener("resize", layout);
  layout();
  const info = await backend.recordingState();
  if (info) update(info);
  // Prepare styles while hidden, then let the native host expose the frame.
  frame.getBoundingClientRect();
  await backend.recordingRegionReady();
  prepared = true;
  if (current) acknowledge(current);
}
