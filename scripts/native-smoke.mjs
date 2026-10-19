// Run with Windows Node after a release build. Uses isolated settings and a solid-color fixture.
import { chromium, expect } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";

if (process.platform !== "win32") throw new Error("Run using Windows Node.js.");
const root = path.resolve(import.meta.dirname, "..");
const exe = path.resolve(
  process.env.FREECUT_NATIVE_EXE ||
    path.join(root, "src-tauri/target/release/FreeCut.exe"),
);
const probe = path.join(
  root,
  "src-tauri/target/debug/examples/native_probe.exe",
);
const output = fs.mkdtempSync(path.join(os.tmpdir(), "freecut-native-"));
const port = 19337;
const env = {
  ...process.env,
  APPDATA: path.join(output, "config"),
  LOCALAPPDATA: path.join(output, "local"),
  WEBVIEW2_USER_DATA_FOLDER: path.join(output, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
};
fs.mkdirSync(env.APPDATA, { recursive: true });
fs.mkdirSync(env.LOCALAPPDATA, { recursive: true });
const probeCall = (...args) =>
  execFileSync(probe, args.map(String), { encoding: "utf8" }).trim();
if (
  process.env.FREECUT_NATIVE_ISOLATED !== "1" &&
  probeCall("running") !== "0"
) {
  throw new Error(
    "Exit the running FreeCut instance before this opt-in native test.",
  );
}
let fixture, app, browser, reservation;
async function connect() {
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
    (page) =>
      !page.url().includes("?overlay") && !page.url().includes("?scroll"),
  );
  const overlay = pages.find((page) => page.url().includes("?overlay"));
  main.on("pageerror", (error) => console.error("Main error:", error));
  overlay.on("pageerror", (error) => console.error("Overlay error:", error));
  await expect(main.locator("#hotkey")).toHaveValue(/Ctrl/);
  return { main, overlay };
}
async function untilVisible(label, expected) {
  await expect
    .poll(() => probeCall("visible", app.pid, label), { timeout: 10000 })
    .toBe(String(expected));
}
async function pixel(page, x, y) {
  return page.locator("#preview-image").evaluate(
    async (img, [x, y]) => {
      const source = new Image();
      source.crossOrigin = "anonymous";
      source.src = img.src;
      await source.decode();
      const c = document.createElement("canvas");
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      const ctx = c.getContext("2d");
      ctx.drawImage(source, 0, 0);
      return {
        width: c.width,
        height: c.height,
        pixel: [
          ...ctx.getImageData(
            Math.floor(x * c.width),
            Math.floor(y * c.height),
            1,
            1,
          ).data,
        ],
      };
    },
    [x, y],
  );
}
async function drawTriangle(overlay) {
  const { w, h } = await overlay.evaluate(() => ({
    w: innerWidth,
    h: innerHeight,
  }));
  await overlay.mouse.move(w / 2 - 100, h / 2 - 80);
  await overlay.mouse.down();
  await overlay.mouse.move(w / 2 + 100, h / 2 - 80, { steps: 15 });
  await overlay.mouse.move(w / 2 - 100, h / 2 + 80, { steps: 15 });
  await overlay.mouse.up();
  await expect(overlay.locator("#selection-toolbar")).toBeVisible();
  await overlay.keyboard.press("Enter");
}
try {
  fixture = spawn(probe, ["fixture"], { stdio: ["ignore", "pipe", "inherit"] });
  await once(fixture.stdout, "data");
  app = spawn(exe, [], { env, stdio: "ignore" });
  let { main, overlay } = await connect();
  await untilVisible("main", true);
  await main.getByRole("button", { name: "开始截图", exact: true }).click();
  await untilVisible("overlay", true);
  await untilVisible("main", false);
  // Check the original frozen pixels where the centered main window was shown.
  // Window visibility alone cannot catch a stale compositor image of the app.
  const frozenCenter = await overlay.evaluate(async () => {
    const session = await window.__TAURI_INTERNALS__.invoke("current_session");
    const source = new Image();
    source.crossOrigin = "anonymous";
    source.src = window.__TAURI_INTERNALS__.convertFileSrc(
      `source-${session.id}`,
      "freecut",
    );
    await source.decode();
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const ctx = canvas.getContext("2d");
    ctx.drawImage(
      source,
      Math.floor(source.naturalWidth / 2),
      Math.floor(source.naturalHeight / 2),
      1,
      1,
      0,
      0,
      1,
      1,
    );
    return [...ctx.getImageData(0, 0, 1, 1).data];
  });
  expect(frozenCenter).toEqual([18, 52, 86, 255]);
  await drawTriangle(overlay);
  await untilVisible("main", true);
  await untilVisible("overlay", false);
  const inside = await pixel(main, 0.1, 0.1),
    outside = await pixel(main, 0.9, 0.9);
  expect(inside.pixel).toEqual([18, 52, 86, 255]);
  expect(outside.pixel).toEqual([0, 0, 0, 0]);
  const scale = await overlay.evaluate(() => devicePixelRatio);
  expect(inside.width).toBe(Math.round(200 * scale));
  expect(inside.height).toBe(Math.round(160 * scale));
  await main.screenshot({ path: path.join(output, "preview.png") });
  console.log(
    "PASS real GDI capture -> lasso -> alpha preview / physical pixels",
  );
  await main.getByRole("button", { name: "复制图片" }).click();
  await expect(main.locator("#status")).toHaveText("图片已复制。");
  probeCall("clipboard", path.join(output, "clipboard.png"));
  console.log("PASS native PNG and DIBV5 clipboard");
  const hotkey = "Ctrl+Alt+Shift+F8";
  await main.locator("#hotkey").click();
  await main.keyboard.press("Control+Alt+Shift+F8");
  await main.getByRole("button", { name: "应用", exact: true }).click();
  await expect(main.locator("#hotkey-status")).toContainText(
    "在其他应用中也能截图",
  );
  reservation = spawn(probe, ["reserve", "Ctrl+Alt+Shift+F9"], {
    stdio: ["pipe", "pipe", "inherit"],
  });
  const [reserved] = await once(reservation.stdout, "data");
  expect(reserved.toString().trim()).toBe("true");
  await main.locator("#hotkey").click();
  await main.keyboard.press("Control+Alt+Shift+F9");
  await main.getByRole("button", { name: "应用", exact: true }).click();
  await expect(main.locator("#hotkey-status")).toContainText("仍有效");
  reservation.stdin.end("\n");
  await once(reservation, "exit");
  reservation = null;
  probeCall("post", app.pid, hotkey);
  await untilVisible("overlay", true);
  await overlay.keyboard.press("Escape");
  await untilVisible("main", true);
  console.log(
    "PASS native hotkey replacement, conflict rollback and WM_HOTKEY delivery",
  );
  await main.getByRole("button", { name: "后台运行" }).click();
  await untilVisible("main", false);
  probeCall("post", app.pid, hotkey);
  await untilVisible("overlay", true);
  probeCall("post", app.pid, hotkey);
  await overlay.keyboard.press("Escape");
  await untilVisible("overlay", false);
  await untilVisible("main", false);
  const second = spawn(exe, [], { env, stdio: "ignore" });
  await once(second, "exit");
  await untilVisible("main", true);
  console.log(
    "PASS background capture, duplicate request, background cancel and single instance",
  );
  await main.getByRole("button", { name: "保存 PNG" }).click();
  await expect
    .poll(
      () => {
        try {
          return probeCall(
            "save-dialog",
            app.pid,
            path.join(output, "saved.png"),
          );
        } catch {
          return null;
        }
      },
      { timeout: 10000 },
    )
    .toBe("saved");
  await expect
    .poll(() => fs.existsSync(path.join(output, "saved.png")))
    .toBe(true);
  await expect(main.locator("#status")).toContainText("已保存");
  expect(
    fs.readFileSync(path.join(output, "saved.png")).subarray(0, 8),
  ).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  console.log("PASS native save dialog -> PNG file");
  const exited = once(app, "exit");
  await main.keyboard.press("Control+q");
  await exited;
  app = null;
  await browser.close();
  browser = null;
  expect(probeCall("check", hotkey)).toBe("true");
  app = spawn(exe, ["--background"], { env, stdio: "ignore" });
  ({ main, overlay } = await connect());
  await untilVisible("main", false);
  await expect(main.locator("#hotkey")).toHaveValue(hotkey);
  probeCall("post", app.pid, hotkey);
  await untilVisible("overlay", true);
  await overlay.keyboard.press("KeyP");
  const size = await overlay.evaluate(() => ({
    w: innerWidth,
    h: innerHeight,
  }));
  for (const [x, y] of [
    [size.w / 2 - 100, size.h / 2 - 80],
    [size.w / 2 + 100, size.h / 2 - 80],
    [size.w / 2 - 100, size.h / 2 + 80],
  ])
    await overlay.mouse.click(x, y);
  await overlay.keyboard.press("Enter");
  await overlay.keyboard.press("Enter");
  await untilVisible("main", true);
  expect((await pixel(main, 0.1, 0.1)).pixel).toEqual([18, 52, 86, 255]);
  console.log(
    "PASS saved hotkey restore, background startup and polygon capture",
  );
  const finalExit = once(app, "exit");
  await main.keyboard.press("Control+q");
  await finalExit;
  app = null;
  console.log(
    `Native smoke tests passed. Controlled fixture artifacts: ${output}`,
  );
} finally {
  if (browser) await browser.close().catch(() => {});
  app?.kill();
  fixture?.kill();
  reservation?.kill();
}
