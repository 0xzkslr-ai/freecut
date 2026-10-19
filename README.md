<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="FreeCut 图标" />
  <h1>FreeCut</h1>
  <p><strong>让截图，自由一点。</strong></p>
  <p>自由套索 · 多边形 · 矩形选取 · 透明 PNG</p>
  <p>
    <a href="https://github.com/0xzkslr-ai/freecut/actions/workflows/ci.yml"><img src="https://github.com/0xzkslr-ai/freecut/actions/workflows/ci.yml/badge.svg" alt="构建与测试" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-087f73" alt="MIT License" /></a>
    <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB" alt="Tauri 2" />
  </p>
</div>

FreeCut 是一款轻量级 Windows 截图工具。你可以像画画一样圈出想要的轮廓，
逐点勾勒多边形，也可以直接框选矩形。选区之外的内容变为透明，结果可保存为 PNG
或复制到剪贴板。

A lightweight Windows screenshot tool with freehand lasso, polygon and rectangle selections, transparent PNG export, global shortcuts and system tray support.

使用 **Tauri 2 + TypeScript + Rust**。本机 Windows x64 发布构建的 exe 约 **3.83 MB**；
体积随编译版本变化，不包含系统 WebView2 Runtime。

## 功能

| 工具     | 选择方式                              | 确认前调整                   |
| -------- | ------------------------------------- | ---------------------------- |
| 自由套索 | 按住左键绘制，松开自动闭合            | 拖动选区移动                 |
| 多边形   | 逐点连线；点击起点、双击或 Enter 闭合 | 移动选区、拖动顶点、撤销顶点 |
| 矩形选取 | 按住左键拖拽，支持任意方向框选        | 移动选区、拖动四个角点缩放   |

- **透明 PNG**：抗锯齿轮廓，保留屏幕原始物理像素；支持凹多边形和自交轮廓。
- **透明预览与复制**：棋盘格显示透明区域；剪贴板同时提供 PNG 和 Windows DIBV5。
- **立即截图**：隐藏主窗口后冻结目标显示器，没有倒计时或人为等待。
- **全局快捷键**：默认 `Ctrl+Alt+A`，支持修改并保存；换键冲突时保留原快捷键。
- **托盘后台**：关闭窗口后驻留托盘；全局键和托盘菜单可直接唤起截图。
- **多显示器选择**：按钮截取选定显示器，全局键截取鼠标所在显示器。
- **单实例运行**：重复启动打开已有实例；后台取消截图后回到后台。

## 获取与运行

当前支持 **Windows 10 / 11 x64**。

从 [GitHub Releases](https://github.com/0xzkslr-ai/freecut/releases/latest) 下载发布版。

项目通过 [GitHub Actions](https://github.com/0xzkslr-ai/freecut/actions/workflows/ci.yml)
构建 Windows 便携版和 NSIS 安装包。构建成功后，可在对应运行页面的 **Artifacts**
中下载 `FreeCut-windows-x64`：

- **便携版**：解压后运行 `FreeCut.exe`，分发时保留同目录的许可文件。
- **安装包**：运行 `FreeCut_*_x64-setup.exe`。
- **从源码运行**：见下方「开发与构建」。

程序使用系统 **WebView2 Runtime**。安装包会在缺失时下载运行时；便携版需要事先
安装运行时。运行时获取方式见 [Tauri Windows 环境说明](https://v2.tauri.app/start/prerequisites/#windows)。

在已经构建的源码目录中，双击 `run.bat` 启动，`run_background.bat` 直接进入托盘。
也可运行 `FreeCut.exe --background`；不会添加开机启动项。

## 使用

1. 在主窗口选择工具和显示器，点击「开始截图」；或按全局键 `Ctrl+Alt+A`，截取鼠标所在显示器。
2. 绘制选区。圈选时按 `L / P / M` 可以切换套索、多边形和矩形。
3. 闭合后拖动选区调整位置；多边形可拖动顶点，矩形可拖动角点调整大小。
4. 按 `Enter` 确认，在预览中复制图片或保存 PNG。`Esc` 取消本次截图。

PNG 的画布是选区的最小外接矩形，轮廓外像素透明。预览棋盘格、操作提示和轮廓线
不会导出。自交轮廓使用奇偶填充规则；粘贴后的透明效果取决于目标软件。

### 快捷键

| 场景   | 快捷键                               | 操作                           |
| ------ | ------------------------------------ | ------------------------------ |
| 全局   | `Ctrl+Alt+A`（可修改）               | 唤起截图                       |
| 主窗口 | `Ctrl+Shift+A`                       | 开始截图                       |
| 主窗口 | `Ctrl+S / Ctrl+C`                    | 保存 / 复制截图                |
| 主窗口 | `Ctrl+Q`                             | 彻底退出                       |
| 圈选   | `L / P / M`                          | 切换套索 / 多边形 / 矩形并重选 |
| 多边形 | `右键 / Backspace / Delete / Ctrl+Z` | 撤销顶点；闭合后先重新打开轮廓 |
| 圈选   | `Enter`                              | 闭合多边形；已闭合时确认       |
| 圈选   | `R / Esc`                            | 重选 / 取消                    |
| 圈选   | `Tab`                                | 隐藏 / 显示提示和工具条        |

修改全局键：点击主窗口的输入框，按新组合，再点击「应用」。支持 Ctrl / Alt
（可搭配 Shift）+ 字母、数字、F1–F24，F12 除外。工具和快捷键设置保存在当前用户的
应用配置目录中。

### 后台行为

- 点击「后台运行」或关闭主窗口后，程序驻留托盘；右键菜单可截图、显示窗口或退出。
- 后台截图按 Esc 返回后台，确认后打开预览。
- 截图或保存对话框打开期间，不会叠加新的截图。
- 没有可用托盘时，不允许隐藏；关闭窗口会退出。
- **更新后测试新版时，请先从托盘退出旧实例，再启动新版。**

## 开发与构建

### Windows 环境

需要：

- Node.js 24（与 CI 一致）。
- Rust stable，使用 `x86_64-pc-windows-msvc` 工具链。
- Visual Studio C++ 生成工具及 Windows SDK。
- WebView2 Runtime。

详细安装步骤见 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/#windows)。
安装 Rust 后重新打开终端。

```powershell
git clone https://github.com/0xzkslr-ai/freecut.git
cd freecut
npm ci
npm run tauri -- dev
```

### 构建 Windows 发布版

双击 `build_windows.bat`，或在 PowerShell 中执行：

```powershell
npm ci
node scripts/licenses.mjs
npm run tauri -- build
node scripts/package-portable.mjs
```

| 输出         | 路径                                    |
| ------------ | --------------------------------------- |
| 便携分发目录 | `dist/FreeCut-Tauri/`                   |
| 编译后的 exe | `src-tauri/target/release/FreeCut.exe`  |
| NSIS 安装包  | `src-tauri/target/release/bundle/nsis/` |

请在 Windows 原生环境中构建和运行 Windows 程序。发布便携版时附带生成的依赖许可
及项目许可；WebView2 使用系统运行时，其安装体积另计。

### 项目结构

前端使用 TypeScript 和 Canvas，Rust 负责截图、蒙版、导出与系统集成。
截图数据通过进程内的自定义协议传给 WebView，不写入临时截图文件。

```text
frontend/
  window.ts       主窗口、工具和设置
  overlay.ts      冻结画面、选区绘制和键鼠输入
  selection.ts    选区状态与几何计算
  bridge.ts       Rust IPC 与图像地址
src-tauri/src/
  desktop.rs      窗口、快捷键、托盘和截图生命周期
  platform.rs     Windows 截图、显示器和剪贴板
  imaging.rs      物理像素裁剪、透明蒙版、PNG 与原子保存
  settings.rs     设置与快捷键校验
tests-web/        前端单元测试和界面流程测试
src-tauri/tests/  Rust 图像处理与设置测试
```

### 测试

```powershell
npm run format:check
npm run build
npm test
npx playwright install chromium
npm run test:e2e
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

前端单元测试覆盖选区几何、三种工具、移动和缩放、边界与快捷键输入。Playwright
通过模拟 Tauri IPC 验证界面流程。Rust 测试验证透明像素、抗锯齿、DPI 坐标转换、
PNG 往返、保存失败保护、设置恢复；Windows 额外验证 DIBV5。

Windows 原生回归脚本可单独运行，需要先构建发布版并退出正在运行的 FreeCut：

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example native_probe --locked
node scripts/native-smoke.mjs
```

脚本使用固定颜色的全屏测试窗口、隔离的设置和临时 WebView2 调试端口，会覆盖剪贴板。
该探针和调试配置不随产品发布。

Linux 可运行前端测试及 Rust 图像处理核心测试：

```sh
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --locked
```

## 当前范围

- 原生截图后端目前仅支持 Windows；Linux / macOS 后端尚未适配。
- 一次截取一个显示器，暂不支持跨屏绘制同一选区。
- HDR 色调映射尚未适配；不同 DPI 的多屏和复杂驱动组合需要更多真机验证。
- 当前没有智能选取、OCR、模型下载或开机启动功能。

## 参与贡献

欢迎通过 [Issues](https://github.com/0xzkslr-ai/freecut/issues) 报告问题或提出建议，
也欢迎提交 Pull Request。

报告截图、缩放或快捷键问题时，请附上 Windows 版本、屏幕分辨率与缩放倍率、
复现步骤和预期结果。修改选区逻辑或图像处理时，请补充对应测试。

## 社区

本开源项目已链接并认可 [LINUX DO](https://linux.do/) 社区。

## 许可

FreeCut 源码采用 [MIT](LICENSE)。第三方依赖保留各自许可，详情见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 和
[依赖许可清单](src-tauri/THIRD_PARTY_LICENSES.txt)。
