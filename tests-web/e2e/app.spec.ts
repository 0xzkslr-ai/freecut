import { test, expect, type Page } from "@playwright/test";

async function mockDesktop(page: Page, overlay = false) {
  await page.addInitScript(
    ({ overlay }) => {
      const win = window as any;
      const callbacks = new Map();
      const listeners = new Map();
      let count = 0;
      win.calls = [];
      win.send = (event: string, payload: unknown) => {
        for (const id of listeners.get(event) ?? [])
          callbacks.get(id)?.({ event, payload });
      };
      win.__TAURI_INTERNALS__ = {
        transformCallback: (callback: unknown) => {
          callbacks.set(++count, callback);
          return count;
        },
        convertFileSrc: (path: string) => `http://freecut.localhost/${path}`,
        invoke: async (cmd: string, args: any) => {
          win.calls.push({ cmd, args });
          if (cmd === "plugin:event|listen") {
            listeners.set(args.event, [
              ...(listeners.get(args.event) ?? []),
              args.handler,
            ]);
            return args.handler;
          }
          if (cmd === "initial_state")
            return {
              settings: { hotkey: "Ctrl+Alt+A", mode: "lasso" },
              monitors: [
                {
                  id: "display1",
                  name: "DISPLAY1",
                  x: 0,
                  y: 0,
                  width: 1920,
                  height: 1080,
                  primary: true,
                },
              ],
              preview: null,
              hotkeyError: null,
              trayAvailable: true,
            };
          if (cmd === "list_monitors")
            return [
              {
                id: "display1",
                name: "DISPLAY1",
                width: 1920,
                height: 1080,
                primary: true,
              },
            ];
          if (cmd === "current_session")
            return overlay
              ? { id: 7, width: 2080, height: 1440, mode: "lasso" }
              : null;
          if (cmd === "set_hotkey") {
            if (args.hotkey === "Ctrl+Alt+X")
              throw "Ctrl+Alt+X 已被占用。原快捷键 Ctrl+Alt+A 仍有效。";
            return args.hotkey;
          }
          if (cmd === "save_image") return "C:\\test\\截图.png";
          if (cmd === "accept_capture") {
            if (win.rejectCrop) throw "选区太小，请重新圈选。";
            return { id: 7, width: 400, height: 400 };
          }
          return null;
        },
      };
    },
    { overlay },
  );
  await page.route("http://freecut.localhost/**", (route) =>
    route.fulfill({
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="2080" height="1440"><rect width="2080" height="1440" fill="#e7f5ef"/><rect x="200" y="200" width="500" height="400" fill="#087f73"/></svg>',
    }),
  );
  await page.goto(overlay ? "/?overlay" : "/");
  await expect
    .poll(() =>
      page.evaluate(
        (overlay) =>
          (window as any).calls.some(
            (c: any) => c.cmd === (overlay ? "show_overlay" : "frontend_ready"),
          ),
        overlay,
      ),
    )
    .toBe(true);
}
const commands = (page: Page, command: string) =>
  page.evaluate(
    (command) =>
      (window as any).calls.filter((call: any) => call.cmd === command),
    command,
  );

test("main flow, tools, capture, preview, save and clipboard", async ({
  page,
}) => {
  await mockDesktop(page);
  await expect(page.getByRole("button", { name: "保存 PNG" })).toBeDisabled();
  await page.getByRole("button", { name: "多边形" }).click();
  await expect(page.getByRole("button", { name: "多边形" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByRole("button", { name: "开始截图", exact: true }).click();
  expect((await commands(page, "start_capture"))[0].args.monitorId).toBe(
    "display1",
  );
  await page.evaluate(() =>
    (window as any).send("preview-changed", { id: 7, width: 400, height: 300 }),
  );
  await expect(page.locator("#dimensions")).toHaveText("400 × 300 px");
  await expect(page.getByAltText("透明截图预览")).toBeVisible();
  await page.getByRole("button", { name: "复制图片" }).click();
  await expect(page.locator("#status")).toHaveText("图片已复制。");
  await page.keyboard.press("Control+s");
  await expect(page.locator("#status")).toContainText("截图.png");
  await page.getByRole("button", { name: "后台运行" }).click();
  expect(await commands(page, "hide_to_tray")).toHaveLength(1);
  await page.keyboard.press("Control+q");
  expect(await commands(page, "quit_app")).toHaveLength(1);
});
test("records new shortcuts, reports conflict and applies a successful replacement", async ({
  page,
}) => {
  await mockDesktop(page);
  const input = page.locator("#hotkey");
  await input.click();
  await page.keyboard.press("Control+Alt+x");
  await page.getByRole("button", { name: "应用", exact: true }).click();
  await expect(page.locator("#hotkey-status")).toContainText(
    "原快捷键 Ctrl+Alt+A 仍有效",
  );
  await input.click();
  await page.keyboard.press("Control+Shift+F8");
  await page.getByRole("button", { name: "应用", exact: true }).click();
  await expect(input).toHaveValue("Ctrl+Shift+F8");
  await expect(page.locator("#hotkey-status")).toContainText(
    "在其他应用中也能截图",
  );
  await input.click();
  await page.keyboard.press("Control+F12");
  await expect(page.locator("#hotkey-status")).toContainText("F12 除外");
});
test("small window has usable controls and a contained preview", async ({
  page,
}) => {
  await page.setViewportSize({ width: 820, height: 600 });
  await mockDesktop(page);
  await expect(page.locator("#capture")).toBeInViewport();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "test-results/main-820.png" });
});
test("lasso closes, moves, toggles HUD and sends only logical points", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.mouse.move(200, 200);
  await page.mouse.down();
  await page.mouse.move(500, 200, { steps: 10 });
  await page.mouse.move(500, 450, { steps: 10 });
  await page.mouse.move(200, 450, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator("#selection-toolbar")).toBeVisible();
  await page.mouse.move(300, 300);
  await page.mouse.down();
  await page.mouse.move(320, 320);
  await page.mouse.up();
  await page.keyboard.press("Tab");
  await expect(page.locator("#hint")).toBeHidden();
  await page.keyboard.press("Tab");
  await page.screenshot({ path: "test-results/overlay-lasso.png" });
  await page.keyboard.press("Enter");
  const call = (await commands(page, "accept_capture"))[0];
  expect(call.args.id).toBe(7);
  expect(call.args.width).toBe(1040);
  expect(call.args.points[0]).toEqual([220, 220]);
});
test("polygon undo, close, vertex edit and cancel", async ({ page }) => {
  await mockDesktop(page, true);
  await page.keyboard.press("KeyP");
  for (const [x, y] of [
    [100, 160],
    [400, 160],
    [400, 400],
  ])
    await page.mouse.click(x, y);
  await page.keyboard.press("Backspace");
  await page.mouse.click(300, 400);
  await page.mouse.click(100, 400);
  await page.keyboard.press("Enter");
  await expect(page.locator("#selection-toolbar")).toBeVisible();
  await page.mouse.move(100, 160);
  await page.mouse.down();
  await page.mouse.move(120, 180);
  await page.mouse.up();
  await page.keyboard.press("Enter");
  expect((await commands(page, "accept_capture"))[0].args.points[0]).toEqual([
    120, 180,
  ]);
});
test("invalid crop stays editable; reset and escape recover", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.evaluate(() => ((window as any).rejectCrop = true));
  await page.mouse.move(100, 160);
  await page.mouse.down();
  await page.mouse.move(400, 160);
  await page.mouse.move(200, 400);
  await page.mouse.up();
  await page.keyboard.press("Enter");
  await expect(page.locator("#hint")).toContainText("选区太小");
  await expect(page.locator("#accept")).toBeEnabled();
  await page.keyboard.press("KeyR");
  await expect(page.locator("#selection-toolbar")).toBeHidden();
  await page.keyboard.press("Escape");
  expect(await commands(page, "cancel_capture")).toHaveLength(1);
});
