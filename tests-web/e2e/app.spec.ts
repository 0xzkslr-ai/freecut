import { test, expect, type Page } from "@playwright/test";

async function mockDesktop(
  page: Page,
  overlay: boolean | "scroll" | "record" | "record-region" = false,
) {
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
          if (cmd === "start_recording") return !win.cancelRecordingDialog;
          if (cmd === "recording_state")
            return {
              id: 7,
              width: 800,
              height: 600,
              region: { x: 120, y: 240, width: 800, height: 600 },
              monitorName: "DISPLAY1",
              elapsedMs: 62_000,
              frames: 1860,
              paused: false,
              starting: overlay === "record-region",
              finishing: false,
              message: "正在录屏",
              hotkey: "Ctrl+Alt+A",
            };
          if (cmd === "pause_recording")
            win.send("recording-progress", {
              id: 7,
              width: 800,
              height: 600,
              region: { x: 120, y: 240, width: 800, height: 600 },
              monitorName: "DISPLAY1",
              elapsedMs: 62_000,
              frames: 1860,
              paused: args.paused,
              starting: false,
              finishing: false,
              message: args.paused ? "已暂停" : "正在录屏",
              hotkey: "Ctrl+Alt+A",
            });
          if (cmd === "start_scroll" && win.rejectScroll)
            throw "采集区域过小，请重新框选。";
          if (cmd === "scroll_state")
            return {
              id: 7,
              width: 1200,
              height: 900,
              frames: 4,
              paused: false,
              message: "正在拼接",
              hotkey: "Ctrl+Alt+A",
            };
          if (cmd === "pause_scroll") {
            win.send("scroll-progress", {
              id: 7,
              width: 1200,
              height: 900,
              frames: 4,
              paused: args.paused,
              message: args.paused ? "已暂停" : "正在拼接",
              hotkey: "Ctrl+Alt+A",
            });
          }
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
  await page.goto(
    overlay === "record-region"
      ? "/?record-region"
      : overlay === "record"
        ? "/?record"
        : overlay === "scroll"
          ? "/?scroll"
          : overlay
            ? "/?overlay"
            : "/",
  );
  await expect
    .poll(() =>
      page.evaluate(
        (overlay) =>
          (window as any).calls.some(
            (c: any) =>
              c.cmd ===
              (overlay === "record-region"
                ? "recording_region_ready"
                : overlay === "record"
                  ? "recording_state"
                  : overlay === "scroll"
                    ? "scroll_state"
                    : overlay
                      ? "show_overlay"
                      : "frontend_ready"),
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

test("recording entry and completed video use the selected monitor and saved path", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "开始录屏", exact: true }).click();
  expect((await commands(page, "start_capture"))[0].args).toEqual({
    monitorId: "display1",
    recording: true,
  });
  await page.evaluate(() =>
    (window as any).send("recording-complete", {
      id: 9,
      width: 800,
      height: 600,
      elapsedMs: 62_000,
      bytes: 2_000_000,
      path: "C:\\test\\录屏.mp4",
      warning: null,
    }),
  );
  await expect(page.locator("#video-result")).toBeVisible();
  await expect(page.locator("#video-details")).toContainText("00:01:02");
  await expect(page.locator("#video-path")).toHaveText("C:\\test\\录屏.mp4");
  await expect(page.locator("#save")).toBeHidden();
  await page.getByRole("button", { name: "打开视频", exact: true }).click();
  expect((await commands(page, "open_recording"))[0].args).toEqual({
    id: 9,
    folder: false,
  });
  await page.evaluate(() =>
    (window as any).send("preview-changed", {
      id: 10,
      width: 400,
      height: 300,
    }),
  );
  await expect(page.locator("#video-result")).toBeHidden();
  await expect(page.locator("#save")).toBeVisible();
});

test("cancelling the recording save dialog retains the rectangle for retry", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.keyboard.press("KeyM");
  await page.mouse.move(200, 200);
  await page.mouse.down();
  await page.mouse.move(500, 450);
  await page.mouse.up();
  await page.evaluate(() => ((window as any).cancelRecordingDialog = true));
  await page.locator("#record-capture").click();
  await expect(page.locator("#record-capture")).toBeVisible();
  await expect(page.locator("#record-capture")).toBeEnabled();
  await page.evaluate(() => ((window as any).cancelRecordingDialog = false));
  await page.locator("#record-capture").click();
  const calls = await commands(page, "start_recording");
  expect(calls).toHaveLength(2);
  expect(calls[1].args.points).toEqual([
    [200, 200],
    [500, 200],
    [500, 450],
    [200, 450],
  ]);
  expect(await commands(page, "accept_capture")).toHaveLength(0);
});

test("recording controls pause, resume and stop once while finalizing", async ({
  page,
}) => {
  await mockDesktop(page, "record");
  await expect(page.locator("#record-time")).toHaveText("00:01:02");
  await expect(page.locator("#record-size")).toHaveText("800 × 600 px");
  await expect(page.locator("#record-position")).toHaveText(
    "DISPLAY1 · 左上角 (120, 240)",
  );
  await page.locator("#record-locate").click();
  expect((await commands(page, "highlight_recording_region"))[0].args).toEqual({
    id: 7,
  });
  await page.getByRole("button", { name: "暂停", exact: true }).click();
  await expect(page.locator("#record-state")).toHaveText("已暂停");
  await expect(page.locator("#app")).toHaveAttribute("data-phase", "paused");
  await page.getByRole("button", { name: "继续", exact: true }).click();
  expect(
    (await commands(page, "pause_recording")).map(
      (call: any) => call.args.paused,
    ),
  ).toEqual([true, false]);
  await page.getByRole("button", { name: "结束并保存", exact: true }).click();
  expect((await commands(page, "finish_recording"))[0].args.id).toBe(7);
  await page.evaluate(() =>
    (window as any).send("recording-progress", {
      id: 7,
      width: 800,
      height: 600,
      elapsedMs: 62_000,
      frames: 1860,
      paused: false,
      starting: false,
      finishing: true,
      message: "正在保存",
      hotkey: "Ctrl+Alt+A",
    }),
  );
  await expect(page.locator("#record-finish")).toBeDisabled();
  await expect(page.locator("#record-cancel")).toBeDisabled();
  await expect(page.locator("#record-locate")).toBeDisabled();
  await expect(page.locator("#app")).toHaveAttribute("data-phase", "finishing");
});

test("recording boundary acknowledges paint and follows pause and completion", async ({
  page,
}) => {
  await mockDesktop(page, "record-region");
  await expect(page.locator(".recording-region")).toBeVisible();
  await expect(page.locator("#region-size")).toHaveText("800 × 600 px");
  await expect
    .poll(async () => (await commands(page, "recording_region_shown")).length)
    .toBe(1);
  expect((await commands(page, "recording_region_shown"))[0].args).toEqual({
    id: 7,
  });
  expect(
    await page
      .locator(".recording-region")
      .evaluate((element) => getComputedStyle(element).pointerEvents),
  ).toBe("none");
  await page.evaluate(() =>
    (window as any).send("recording-progress", {
      id: 7,
      width: 800,
      height: 600,
      region: { x: 120, y: 240, width: 800, height: 600 },
      elapsedMs: 62_000,
      frames: 1860,
      paused: true,
      starting: false,
      finishing: false,
      message: "已暂停",
      hotkey: "Ctrl+Alt+A",
    }),
  );
  await expect(page.locator("#region-state")).toHaveText("已暂停");
  await expect(page.locator("#app")).toHaveAttribute("data-phase", "paused");
  await expect(page.locator(".recording-region")).toHaveCSS(
    "border-top-style",
    "dashed",
  );
  await page.evaluate(() => (window as any).send("recording-highlight", 99));
  await expect(page.locator("#app")).not.toHaveClass(/region-highlight/);
  await page.evaluate(() => (window as any).send("recording-highlight", 7));
  await expect(page.locator("#app")).toHaveClass(/region-highlight/);
  await page.evaluate(() => (window as any).send("recording-ended", null));
  await expect(page.locator("#region-size")).toHaveText("");
  await expect(page.locator("#app")).not.toHaveClass(/region-highlight/);
});

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

test("scroll capture is offered only for a closed rectangle and sends the selected region", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.mouse.move(200, 200);
  await page.mouse.down();
  await page.mouse.move(500, 200);
  await page.mouse.move(200, 450);
  await page.mouse.up();
  await expect(page.locator("#scroll-capture")).toBeHidden();
  await page.keyboard.press("KeyM");
  await page.mouse.move(200, 200);
  await page.mouse.down();
  await page.mouse.move(500, 450);
  await page.mouse.up();
  await expect(page.locator("#scroll-capture")).toBeVisible();
  await page.evaluate(() => ((window as any).rejectScroll = true));
  await page.locator("#scroll-capture").click();
  await expect(page.locator("#hint")).toContainText("采集区域过小");
  await expect(page.locator("#scroll-capture")).toBeEnabled();
  await page.evaluate(() => ((window as any).rejectScroll = false));
  await page.locator("#scroll-capture").click();
  expect((await commands(page, "start_scroll"))[1].args.points).toEqual([
    [200, 200],
    [500, 200],
    [500, 450],
    [200, 450],
  ]);
  expect(await commands(page, "accept_capture")).toHaveLength(0);
});

test("scroll controls show progress, pause, resume and finish", async ({
  page,
}) => {
  await mockDesktop(page, "scroll");
  await expect(page.locator("#scroll-size")).toHaveText("1200 × 900 px · 4 帧");
  await expect(page.locator("#scroll-hotkey")).toContainText("Ctrl+Alt+A");
  await page.getByRole("button", { name: "暂停", exact: true }).click();
  await expect(page.locator("#scroll-message")).toHaveText("已暂停");
  await page.getByRole("button", { name: "继续采集", exact: true }).click();
  expect(
    (await commands(page, "pause_scroll")).map((call: any) => call.args.paused),
  ).toEqual([true, false]);
  await page.evaluate(() =>
    (window as any).send("scroll-progress", {
      id: 7,
      width: 2000,
      height: 1500,
      frames: 8,
      paused: true,
      message: "无法可靠对齐，已暂停",
      hotkey: "Ctrl+Alt+A",
    }),
  );
  await expect(page.locator("#scroll-message")).toContainText("无法可靠对齐");
  await page.getByRole("button", { name: "结束并预览", exact: true }).click();
  expect((await commands(page, "finish_scroll"))[0].args.id).toBe(7);
  await page.evaluate(() => (window as any).send("scroll-ended", null));
  await page.getByRole("button", { name: "结束并预览", exact: true }).click();
  expect(await commands(page, "finish_scroll")).toHaveLength(1);
});

test("cancel discards scrolling and a preview can be reopened for cropping", async ({
  page,
}) => {
  await mockDesktop(page, "scroll");
  await page.getByRole("button", { name: "取消", exact: true }).click();
  expect((await commands(page, "cancel_scroll"))[0].args.id).toBe(7);
  await page.goto("/");
  await expect(page.locator("#edit")).toBeDisabled();
  await page.evaluate(() =>
    (window as any).send("preview-changed", {
      id: 9,
      width: 10000,
      height: 1000,
    }),
  );
  await page.getByRole("button", { name: "再次裁剪", exact: true }).click();
  expect(await commands(page, "edit_result")).toHaveLength(1);
});

test("a stitched image fits the crop window and maps its selection without stretching", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.evaluate(() =>
    (window as any).send("capture-started", {
      id: 9,
      width: 10000,
      height: 1000,
      mode: "rectangle",
      crop: true,
    }),
  );
  await expect
    .poll(async () => (await commands(page, "show_overlay")).length)
    .toBe(2);
  // 10000 × 1000 is fitted to 992 × 99.2 at (24, 310.4).
  await page.mouse.move(24, 310.4);
  await page.mouse.down();
  await page.mouse.move(520, 409.6);
  await page.mouse.up();
  await expect(page.locator("#selection-size")).toHaveText("约 5000 × 1000 px");
  await expect(page.locator("#scroll-capture")).toBeHidden();
  await page.keyboard.press("Enter");
  const points = (await commands(page, "accept_capture"))[0].args.points;
  expect(points[0][0]).toBeCloseTo(0);
  expect(points[2][0]).toBeCloseTo(520);
  expect(points[2][1]).toBeCloseTo(720);
});
test("rectangle tool is selected and persisted through the backend", async ({
  page,
}) => {
  await mockDesktop(page);
  const button = page.getByRole("button", { name: "矩形选取" });
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  expect((await commands(page, "set_mode"))[0].args.mode).toBe("rectangle");
  await expect(page.getByRole("button", { name: "自由套索" })).toHaveAttribute(
    "aria-pressed",
    "false",
  );
});
test("rectangle drag, move, corner resize and confirmation keep four axis-aligned points", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.keyboard.press("KeyM");
  await expect(page.locator("#hint")).toContainText("矩形选取");
  expect((await commands(page, "set_mode"))[0].args.mode).toBe("rectangle");
  await page.mouse.move(500, 450);
  await page.mouse.down();
  await page.mouse.move(200, 200, { steps: 5 });
  await page.mouse.up();
  await expect(page.locator("#selection-toolbar")).toBeVisible();
  await expect(page.locator("#selection-size")).toHaveText("约 600 × 500 px");
  await page.mouse.move(300, 300);
  await page.mouse.down();
  await page.mouse.move(320, 320);
  await page.mouse.up();
  await page.mouse.move(520, 470);
  await page.mouse.down();
  await page.mouse.move(600, 500);
  await page.mouse.up();
  await expect(page.locator("#selection-size")).toHaveText("约 760 × 560 px");
  await page.screenshot({ path: "test-results/overlay-rectangle.png" });
  await page.keyboard.press("Enter");
  expect((await commands(page, "accept_capture"))[0].args.points).toEqual([
    [220, 220],
    [600, 220],
    [600, 500],
    [220, 500],
  ]);
});
test("empty rectangles are rejected; undo and escape leave the overlay usable", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await page.keyboard.press("KeyM");
  await page.keyboard.press("Enter");
  await expect(page.locator("#hint")).toContainText("宽度和高度");
  await page.mouse.click(200, 200);
  await expect(page.locator("#hint")).toContainText("宽度和高度");
  await expect(page.locator("#selection-toolbar")).toBeHidden();
  await page.mouse.move(200, 200);
  await page.mouse.down();
  await page.mouse.move(500, 400);
  await page.mouse.up();
  await expect(page.locator("#selection-toolbar")).toBeVisible();
  await page.keyboard.press("Backspace");
  await expect(page.locator("#selection-toolbar")).toBeHidden();
  await page.keyboard.press("Escape");
  expect(await commands(page, "cancel_capture")).toHaveLength(1);
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
