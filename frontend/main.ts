import "./style.css";
import { mountMain } from "./window";
import { mountOverlay } from "./overlay";
import { mountScroll } from "./scroll";
import { backend } from "./bridge";

const root = document.querySelector<HTMLDivElement>("#app")!;
const isOverlay = new URLSearchParams(location.search).has("overlay");
const isScroll = new URLSearchParams(location.search).has("scroll");
document.body.classList.toggle("overlay-window", isOverlay);
document.body.classList.toggle("scroll-window", isScroll);
(isOverlay
  ? mountOverlay(root)
  : isScroll
    ? mountScroll(root)
    : mountMain(root)
).catch((error: unknown) => {
  const message = document.createElement("p");
  message.className = "fatal";
  message.textContent = `FreeCut 初始化失败：${String(error)}。请关闭后重新启动。`;
  root.replaceChildren(message);
  const detail =
    error instanceof Error ? (error.stack ?? error.message) : String(error);
  void backend.frontendFailed(detail).catch(() => {
    // Native startup watchdog remains available if the IPC bridge also fails.
  });
});
