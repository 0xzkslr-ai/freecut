<div align="center">
  <img src="src-tauri/icons/128x128.png" width="80" height="80" alt="FreeCut 图标" />
  <h1>FreeCut</h1>
  <p><strong>让截图，自由一点。</strong></p>
  <p>自由套索 · 多边形 · 矩形选取 · 滚动长截图 · 透明 PNG</p>
  <p>
    <a href="https://github.com/0xzkslr-ai/freecut/actions/workflows/ci.yml"><img src="https://github.com/0xzkslr-ai/freecut/actions/workflows/ci.yml/badge.svg" alt="构建与测试" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-087f73" alt="MIT License" /></a>
    <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB" alt="Tauri 2" />
  </p>
</div>

FreeCut 是一款轻量级 Windows 截图工具。你可以像画画一样圈出想要的轮廓，
逐点勾勒多边形，也可以直接框选矩形。选区之外的内容变为透明，结果可保存为 PNG
或复制到剪贴板。矩形选区还支持滚动拼接：手动滚动页面或移动地图，将屏幕之外的内容逐步拼成大图。

A lightweight Windows screenshot tool with freehand lasso, polygon and rectangle selections, scrolling capture, transparent PNG export, global shortcuts and system tray support.

使用 **Tauri 2 + TypeScript + Rust**。本机 Windows x64 发布构建的 exe 约 **4.76 MB**；
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
- **滚动拼接**：矩形框定地图区域，你手动上下左右移动地图，工具自动对齐、拼接；支持暂停、继续及全局键结束。
- **再次裁剪**：拼接图或普通截图均可再次用套索、多边形、矩形裁剪，保留原始像素和透明区域。
- **区域录屏**：矩形框选后录制画面和鼠标指针，支持暂停、继续、快捷键结束，保存为 MP4。

**v0.3.1 新增启动诊断日志、WebView2 检测和启动失败弹窗**，并修复滚动拼接在返回和换向时可能出现的像素偏移。

## 截图效果

以下是使用 FreeCut 实际导出的 PNG，点击图片可查看原图。

| 自由套索 | 多边形 | 矩形选取 |
| :---: | :---: | :---: |
| [<img src="screenshots/lasso/FreeCut-1792423387.png" width="240" alt="自由套索截图：沿人物轮廓手绘选区" />](screenshots/lasso/FreeCut-1792423387.png) | [<img src="screenshots/polygon/FreeCut-1792423427.png" width="240" alt="多边形截图：逐点勾勒视频画面选区" />](screenshots/polygon/FreeCut-1792423427.png) | [<img src="screenshots/rectangle/FreeCut-1792423459.png" width="240" alt="矩形截图：框选视频画面" />](screenshots/rectangle/FreeCut-1792423459.png) |
| 按住鼠标圈出想要的轮廓 | 逐点选择，保留多边形范围 | 拖动框选，快速截取矩形区域 |

### 滚动长截图

网页评论区的滚动拼接示例，原图为 **1286 × 4015 px**。矩形框定采集区域后，手动滚动页面，结束后得到完整拼接结果。

<details>
<summary>展开查看长截图</summary>

[<img src="screenshots/scroll/FreeCut-1792423560.png" width="480" alt="滚动长截图：将网页评论区的多屏内容拼接成一张长图" />](screenshots/scroll/FreeCut-1792423560.png)

</details>

更多样例与分类说明见 [截图样例目录](screenshots/README.md)。

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

启动诊断日志保存在 `%LOCALAPPDATA%\FreeCut\logs\`，每次启动生成一个
`startup-*.log`，保留最近 10 份，每份最多约 256 KB。目录无法写入时回退到
`%TEMP%\FreeCut\logs\`。日志记录版本、Windows 版本、WebView2 检测结果、
窗口加载和初始化错误，不记录截图像素或剪贴板内容。
缺少运行时、初始化失败或主界面超过 30 秒未就绪时，会显示原生错误弹窗并附上
实际日志路径。反馈“无法启动”时，请提供对应日志；启动超时提示后程序仍可继续初始化。

## 使用

1. 在主窗口选择工具和显示器，点击「开始截图」；或按全局键 `Ctrl+Alt+A`，截取鼠标所在显示器。
2. 绘制选区。圈选时按 `L / P / M` 可以切换套索、多边形和矩形。
3. 闭合后拖动选区调整位置；多边形可拖动顶点，矩形可拖动角点调整大小。
4. 按 `Enter` 确认，在预览中复制图片或保存 PNG。`Esc` 取消本次截图。

### 区域录屏

1. 点击「开始录屏」，拖动矩形选择区域，再按 `Enter`；也可在普通矩形选区中点击「录屏」。
2. 选择 MP4 保存位置后开始录制。取消保存对话框会保留选区，可以调整后重试。
3. 录制范围始终显示红色边框，暂停后变为琥珀色虚线并显示「已暂停」。边框支持鼠标穿透，可以直接操作区域内的软件。
4. 悬浮控制窗显示时长、区域尺寸和位置，支持暂停、继续和「结束并保存」。点击「定位范围」可加粗高亮边框；拖动标题可以移动控制窗。
5. 按当前全局截图快捷键结束录屏；控制窗内按空格暂停或继续。完成后主窗口显示文件路径、时长和大小，可直接打开视频或所在文件夹。

首版录制画面与鼠标指针，无音频。使用 Windows 自带 H.264 编码器，优先尝试硬件编码，
不可用时尝试系统软件编码；媒体组件只在开始录屏时加载。控制窗和范围边框从画面中排除，
因此录屏需要 Windows 10 2004 或更新系统。
目标帧率为 30 FPS，实际采集速度取决于区域大小、编码器和磁盘性能；暂停时间不计入视频。
支持宽高至少 48 像素、最大 3840 × 2160 像素的区域。为兼容 H.264，奇数宽高会在边缘补齐 1 像素。
录制期间逐帧写入保存目录的临时文件，正常结束后替换为所选文件名；取消录制不会改动已有目标文件。
录屏时选择退出会先结束并保存 MP4，再退出程序；保存失败时保留窗口显示错误。

PNG 的画布是选区的最小外接矩形，轮廓外像素透明。预览棋盘格、操作提示和轮廓线
不会导出。自交轮廓使用奇偶填充规则；粘贴后的透明效果取决于目标软件。

### 滚动截图与地图拼接

1. 选择「矩形选取」，开始截图后框定地图显示区域，避开固定按钮、侧栏和小地图。
2. 闭合选区后点击工具条上的「滚动截图」。采集立即开始，你可以正常操作原来的地图。
3. 保持地图缩放和方向不变，缓慢滚动或拖动，邻近画面尽量保留至少一半重叠。支持上下左右、斜向和反向移动。
4. 控制窗始终置顶，显示拼接尺寸和覆盖图：绿色为已采集区域，实线框为当前位置，定位中使用虚线显示最后一次位置，棋盘格为空白。可暂停或继续，拖动标题可以移动窗口。控制窗优先放在选区外；即使选区覆盖全屏，控制窗也会保持显示，并从截图中排除，不会进入拼接图片。这需要 Windows 10 2004 或更新系统支持，使用 Windows 的[窗口捕获排除接口](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)。
5. 再按一次全局截图快捷键（默认 `Ctrl+Alt+A`），或点击「结束并预览」／托盘「结束滚动截图」。预览中可直接复制、保存，也可点击「再次裁剪」后用任意现有工具裁剪。

短暂无法匹配时，工具继续采集并尝试与历史关键画面重新定位，不会自动弹窗抢走操作焦点。
可以放慢移动，或返回已经经过的区域帮助恢复定位；未定位的缓存片段不会被强行拼接。
读写失败或尺寸达到上限时会暂停并显示原因，之前的结果会保留。结束后在后台校正关键画面的位置、合成原始像素，再打开预览。
「取消」会丢弃本次拼接，保留之前的截图预览。

工具只能拼接实际显示并采集过的内容，无法判断游戏地图的完整边界。请自行移动视角覆盖需要的全部区域；
未经过的区域保持透明。当前支持固定缩放、固定方向的平面内容，不适用于旋转／缩放变化、3D 透视变化、
大面积动态特效或重复且无法区分的地形。拼接图上限为 4000 万像素，单边不超过 32768 像素。
关键画面使用临时 PNG 缓存，正常结束或取消后自动清理；磁盘缓存上限为 512 MB，最多保留 1024 张关键画面。
连续采集缓冲、历史画面缓存和待定位片段均设有数量及内存预算，大图会增加实际内存占用。

### 快捷键

| 场景   | 快捷键                               | 操作                           |
| ------ | ------------------------------------ | ------------------------------ |
| 全局   | `Ctrl+Alt+A`（可修改）               | 唤起截图                       |
| 滚动采集 | 当前全局截图快捷键                 | 结束拼接并预览                 |
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
截图数据通过进程内的自定义协议传给 WebView。滚动采集的关键画面单独存入有上限的临时缓存，以便重新定位和最终合成。

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
  stitching.rs    固定缩放图像对齐、二维拼接与尺寸限制
  scrolling.rs    连续采集线程、暂停恢复和控制窗口
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
node scripts/native-scroll-smoke.mjs
```

脚本使用固定颜色的全屏测试窗口、隔离的设置和临时 WebView2 调试端口，会覆盖剪贴板。
该探针和调试配置不随产品发布。

滚动回归脚本使用可平移的纹理地图，验证真实 Windows 连续截图、四向拼接、匹配失败后的恢复、
全局键结束、原始像素、透明裁剪及全屏采集时控制窗持续显示。运行时会打开测试窗口，请在空闲桌面执行。

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
