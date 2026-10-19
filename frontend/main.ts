import "./style.css";
import { mountMain } from "./window";
import { mountOverlay } from "./overlay";

const root = document.querySelector<HTMLDivElement>("#app")!;
const isOverlay = new URLSearchParams(location.search).has("overlay");
document.body.classList.toggle("overlay-window", isOverlay);
(isOverlay ? mountOverlay(root) : mountMain(root)).catch((error: unknown) => {
  const message = document.createElement("p");
  message.className = "fatal";
  message.textContent = `FreeCut 初始化失败：${String(error)}。请关闭后重新启动。`;
  root.replaceChildren(message);
});
