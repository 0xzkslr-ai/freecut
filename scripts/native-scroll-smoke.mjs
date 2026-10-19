// Opt-in real Windows capture test. Use a separately built application identifier
// when testing alongside an existing instance; never bundle this script or fixture.
import { chromium, expect } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";

if (process.platform !== "win32") throw new Error("Run using Windows Node.js.");
const root = path.resolve(import.meta.dirname, "..");
const exe = path.resolve(
  process.env.FREECUT_NATIVE_EXE || "src-tauri/target/release/FreeCut.exe",
);
const probe = path.join(
  root,
  "src-tauri/target/debug/examples/native_probe.exe",
);
const output = fs.mkdtempSync(path.join(os.tmpdir(), "freecut-scroll-"));
const port = 19338;
const probeCall = (...args) =>
  execFileSync(probe, args.map(String), { encoding: "utf8" }).trim();
if (process.env.FREECUT_NATIVE_ISOLATED !== "1" && probeCall("running") !== "0")
  throw new Error(
    "Exit the running FreeCut instance before this opt-in native test.",
  );
let app, fixture, browser;
let restore = async () => {};
try {
  fixture = spawn(probe, ["map-fixture"], {
    stdio: ["ignore", "pipe", "inherit"],
  });
  await once(fixture.stdout, "data");
  app = spawn(exe, [], {
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: path.join(output, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: "ignore",
  });
  await expect
    .poll(
      async () => {
        try {
          return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;
        } catch {
          return false;
        }
      },
      { timeout: 20000 },
    )
    .toBe(true);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await expect.poll(() => browser.contexts()[0].pages().length).toBe(3);
  await expect
    .poll(
      () =>
        browser
          .contexts()[0]
          .pages()
          .filter(
            (page) =>
              page.url().includes("?overlay") || page.url().includes("?scroll"),
          ).length,
    )
    .toBe(2);
  const pages = browser.contexts()[0].pages();
  const main = pages.find(
    (p) => !p.url().includes("?overlay") && !p.url().includes("?scroll"),
  );
  const overlay = pages.find((p) => p.url().includes("?overlay"));
  const panel = pages.find((p) => p.url().includes("?scroll"));
  for (const page of pages)
    page.on("pageerror", (error) => console.error(error));
  const invoke = (page, command, args) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const state = () => invoke(panel, "scroll_state");
  const originalSettings = (await invoke(main, "initial_state")).settings;
  restore = async () => {
    const scrolling = await state();
    if (scrolling) await invoke(panel, "cancel_scroll", { id: scrolling.id });
    const capture = await invoke(overlay, "current_session");
    if (capture) await invoke(overlay, "cancel_capture", { id: capture.id });
    await invoke(main, "set_mode", { mode: originalSettings.mode });
    await invoke(main, "set_hotkey", { hotkey: originalSettings.hotkey });
  };
  const hotkey = "Ctrl+Alt+Shift+F10";
  await main.locator("#hotkey").click();
  await main.keyboard.press("Control+Alt+Shift+F10");
  await main.locator("#apply-hotkey").click();
  await expect(main.locator("#hotkey-status")).toContainText(
    "在其他应用中也能截图",
  );
  await main.getByRole("button", { name: "矩形选取" }).click();
  await main.locator("#capture").click();
  await expect
    .poll(() => probeCall("visible", app.pid, "overlay"))
    .toBe("true");
  expect(probeCall("visible", app.pid, "main")).toBe("false");
  const scale = await overlay.evaluate(() => devicePixelRatio);
  await overlay.mouse.move(100, 160);
  await overlay.mouse.down();
  await overlay.mouse.move(600, 460);
  await overlay.mouse.up();
  await overlay.locator("#scroll-capture").click();
  await expect.poll(async () => (await state())?.frames).toBe(1);
  const first = await state();
  if (first.paused) {
    console.error("Initial scroll state:", first, "DPI:", scale);
    await invoke(panel, "finish_scroll", { id: first.id });
    await main.screenshot({ path: path.join(output, "initial-failure.png") });
    throw new Error(`Initial capture paused; fixture screenshot: ${output}`);
  }
  expect(first.paused).toBe(false);
  for (const [x, y] of [
    [80, 0],
    [80, 60],
    [0, 60],
    [0, 0],
  ]) {
    const before = (await state()).frames;
    probeCall("map-move", fixture.pid, x, y);
    await expect
      .poll(async () => (await state()).frames, { timeout: 15000 })
      .toBe(before + 1);
  }
  const stitched = await state();
  expect(stitched.width).toBe(first.width + 80);
  expect(stitched.height).toBe(first.height + 60);
  console.log(
    "PASS real GDI capture -> four-direction stitching -> return to origin",
  );
  await invoke(panel, "pause_scroll", { id: first.id, paused: true });
  probeCall("map-move", fixture.pid, 1000, 1000);
  await invoke(panel, "pause_scroll", { id: first.id, paused: false });
  await expect
    .poll(async () => (await state()).paused, { timeout: 15000 })
    .toBe(true);
  await expect(panel.locator("#scroll-message")).toContainText("无法可靠对齐");
  probeCall("map-move", fixture.pid, 0, 0);
  await panel.getByRole("button", { name: "继续采集" }).click();
  await expect.poll(async () => (await state()).paused).toBe(false);
  probeCall("post", app.pid, hotkey);
  await expect.poll(state).toBe(null);
  await expect(main.locator("#dimensions")).toHaveText(
    `${stitched.width} × ${stitched.height} px`,
  );
  const pixels = await main.locator("#preview-image").evaluate(async (img) => {
    const source = new Image();
    source.crossOrigin = "anonymous";
    source.src = img.src;
    await source.decode();
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext("2d");
    ctx.drawImage(source, 0, 0);
    return [
      [0, 0],
      [canvas.width - 1, canvas.height - 1],
    ].map(([x, y]) => [...ctx.getImageData(x, y, 1, 1).data]);
  });
  function expectedPixel(x, y) {
    let n =
      Math.imul(Math.floor(x / 11), 0x45d9f3b) ^
      Math.imul(Math.floor(y / 13), 0x119de1f3);
    n = Math.imul(n ^ (n >>> 16), 0x45d9f3b);
    const detail = (x * 17 + y * 29) % 29;
    return [(n & 255) >>> 1, ((n >>> 8) & 255) >>> 1, ((n >>> 16) & 255) >>> 1]
      .map((v) => v + detail)
      .concat(255);
  }
  expect(pixels[0]).toEqual(
    expectedPixel(Math.floor(100 * scale), Math.floor(160 * scale)),
  );
  expect(pixels[1]).toEqual(
    expectedPixel(
      Math.floor(100 * scale) + stitched.width - 1,
      Math.floor(160 * scale) + stitched.height - 1,
    ),
  );
  console.log(
    "PASS mismatch pauses without losing image; resume -> global shortcut finish -> exact exported pixels",
  );
  await main.locator("#edit").click();
  await expect
    .poll(() => probeCall("visible", app.pid, "overlay"))
    .toBe("true");
  await overlay.keyboard.press("KeyP");
  const view = await overlay.evaluate(async () => {
    const session = await window.__TAURI_INTERNALS__.invoke("current_session");
    const scale = Math.min(
      (innerWidth - 48) / session.width,
      (innerHeight - 120) / session.height,
    );
    return {
      x: (innerWidth - session.width * scale) / 2,
      y: (innerHeight - session.height * scale) / 2,
      width: session.width * scale,
      height: session.height * scale,
    };
  });
  for (const [x, y] of [
    [view.x + 10, view.y + 10],
    [view.x + view.width - 10, view.y + 10],
    [view.x + 10, view.y + view.height - 10],
  ])
    await overlay.mouse.click(x, y);
  await overlay.keyboard.press("Enter");
  await overlay.keyboard.press("Enter");
  await expect.poll(() => probeCall("visible", app.pid, "main")).toBe("true");
  const alpha = await main.locator("#preview-image").evaluate(async (img) => {
    const source = new Image();
    source.crossOrigin = "anonymous";
    source.src = img.src;
    await source.decode();
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const ctx = c.getContext("2d");
    ctx.drawImage(source, 0, 0);
    return ctx.getImageData(c.width - 1, c.height - 1, 1, 1).data[3];
  });
  expect(alpha).toBe(0);
  console.log(
    "PASS stitched image -> fitted polygon crop -> transparent preview",
  );
  await main.screenshot({ path: path.join(output, "cropped-map.png") });
  await main.locator("#capture").click();
  await expect
    .poll(() => probeCall("visible", app.pid, "overlay"))
    .toBe("true");
  const session = await invoke(overlay, "current_session");
  const size = await overlay.evaluate(() => ({
    width: innerWidth,
    height: innerHeight,
  }));
  await invoke(overlay, "start_scroll", {
    id: session.id,
    width: size.width,
    height: size.height,
    points: [
      [0, 0],
      [size.width, 0],
      [size.width, size.height],
      [0, size.height],
    ],
  });
  await expect.poll(() => probeCall("visible", app.pid, "scroll")).toBe("true");
  const second = spawn(exe, [], { stdio: "ignore" });
  await once(second, "exit");
  await expect.poll(async () => (await state()).paused).toBe(true);
  await expect.poll(() => probeCall("visible", app.pid, "scroll")).toBe("true");
  await invoke(panel, "pause_scroll", { id: session.id, paused: false });
  await expect.poll(() => probeCall("visible", app.pid, "scroll")).toBe("true");
  probeCall("post", app.pid, hotkey);
  await expect.poll(state).toBe(null);
  console.log(
    "PASS full-monitor controls remain visible through resume; global shortcut finishes",
  );
  await restore();
  restore = async () => {};
  const exited = once(app, "exit");
  await main.keyboard.press("Control+q");
  await exited;
  app = null;
  console.log(`Native scroll smoke tests passed: ${output}`);
} finally {
  await restore().catch(() => {});
  if (browser) await browser.close().catch(() => {});
  app?.kill();
  fixture?.kill();
}
