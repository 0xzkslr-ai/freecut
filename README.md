# FreeCut

自由形状截图工具。支持自由套索、多边形和传统矩形选取，再保存为 PNG。

使用 **Tauri 2 + TypeScript + Rust**。
截图后端当前支持 Windows 10 / 11 x64；Linux / macOS 的原生截图尚未迁移。

## 使用

双击 `dist\FreeCut-Tauri\FreeCut.exe`，或运行根目录的 `run.bat`。
`run_background.bat` 直接启动到托盘，也可以给 exe 添加 `--background` 参数。
需要系统安装 WebView2 Runtime；安装包会在缺失时下载运行时，不内置整套浏览器。
便携版首次运行前需自行安装缺失的运行时。

1. 选择「自由套索」「多边形」或「矩形选取」和目标显示器，点击「开始截图」。
2. 主窗口隐藏后立即冻结屏幕，没有倒计时或人为等待。
3. 套索按住左键绘制、松开闭合；多边形逐点添加，点击起点 / 双击 / Enter 闭合；矩形按住左键拖拽、松开完成。
4. 拖动闭合选区移动；多边形可以拖动顶点调整，矩形可以拖动四个角点调整大小。
5. Enter 确认，查看透明预览，然后复制图片或保存 PNG。

选区之外的像素透明，PNG 画布是选区最小外接矩形。导出使用屏幕物理像素，
支持抗锯齿边缘、凹多边形和奇偶规则的自交轮廓；棋盘格和选区线不会导出。
剪贴板同时提供 PNG 和 Windows DIBV5 图片，目标软件对透明度的支持可能不同。
一次截取一个显示器，不支持跨屏绘制；HDR 画面的色调映射尚未适配。

当前不包含智能选取、OCR、模型下载或开机启动。

## 快捷键与后台

- 默认全局截图键 **Ctrl+Alt+A**。点击输入框，按新组合，再点击「应用」。
  支持 Ctrl / Alt（可搭配 Shift）+ 字母、数字、F1–F24，F12 除外。
- 更换失败会保留旧快捷键。快捷键和工具设置保存到当前用户的应用配置目录。
- 关闭主窗口或点击「后台运行」后驻留托盘，右键菜单可截图、显示窗口或退出。
  没有可用托盘时不允许隐藏，关闭窗口会退出。
- 全局键和托盘菜单截取鼠标所在显示器；主窗口按钮截取下拉框选定的显示器。
- 后台截图按 Esc 返回后台；确认后打开预览。截图和保存对话框打开期间不会叠加截图。
- 重复启动会打开已有实例。彻底退出请用托盘菜单或 Ctrl+Q。

| 场景 | 快捷键 | 操作 |
| --- | --- | --- |
| 全局 | Ctrl+Alt+A（可修改） | 唤起截图 |
| 主窗口 | Ctrl+Shift+A | 开始截图 |
| 主窗口 | Ctrl+S / Ctrl+C | 保存 / 复制截图 |
| 主窗口 | Ctrl+Q | 彻底退出 |
| 圈选 | L / P / M | 切换套索 / 多边形 / 矩形并重选 |
| 多边形 | 右键 / Backspace / Delete / Ctrl+Z | 撤销顶点；闭合后先重新打开轮廓 |
| 圈选 | Enter | 闭合多边形；已闭合时确认 |
| 圈选 | R / Esc | 重选 / 取消 |
| 圈选 | Tab | 隐藏 / 显示操作提示和工具条 |

## 开发

Windows 需要 Node.js 22.12+（或更新 LTS）、Rust stable MSVC、Visual Studio C++
生成工具及 Windows SDK、WebView2。安装 Rust 后重新打开终端。

```powershell
npm ci
npm run tauri -- dev
```

前端为 TypeScript + Canvas；Rust 负责 GDI 截屏、tiny-skia 蒙版、PNG、原生剪贴板、
保存、快捷键、窗口与托盘调度。截图数据通过进程内的自定义协议传给 WebView，
不写入临时截图文件。

```text
frontend/
  window.ts      主窗口与设置
  overlay.ts     冻结画面、选区绘制和输入
  selection.ts   选区状态、撤销、移动和几何计算
  bridge.ts      Rust IPC 与图像地址
src-tauri/src/
  desktop.rs     窗口、快捷键、托盘和截图生命周期
  platform.rs    Windows 截屏、显示器和剪贴板
  imaging.rs     物理像素裁剪、透明蒙版和原子保存
  settings.rs    设置与快捷键校验
```

## 验证

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

前端单元测试覆盖几何、套索、多边形、移动边界和快捷键输入。Playwright 通过模拟
Tauri IPC 验证界面流程；不代表原生截屏测试。Rust 测试验证透明像素、抗锯齿、DPI
映射、PNG 往返、保存失败保护、设置恢复，Windows 额外验证 DIBV5。
Linux 可运行前端测试和 `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`。
不同 DPI 的多屏、HDR 和复杂驱动组合仍需要更多真机验证。

Windows 真机回归（先退出正在运行的 FreeCut）：

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example native_probe --locked
node scripts/native-smoke.mjs
```

该测试需要先构建发布版；使用隔离的设置、固定颜色的全屏测试窗口和临时 WebView2
调试端口，验证真实截图、透明像素、快捷键消息、后台、剪贴板、原生保存和重启。
会暂时覆盖剪贴板图片，结束后关闭测试进程。该探针和调试设置不随产品发布。

## Windows 打包

双击 `build_windows.bat`，或运行：

```powershell
npm ci
node scripts/licenses.mjs
npm run tauri -- build
```

`src-tauri\target\release\FreeCut.exe` 为便携程序；
`src-tauri\target\release\bundle\nsis\` 为安装包。
脚本会将程序和许可整理到 `dist\FreeCut-Tauri\`。
WebView2 使用系统运行时，其安装体积另计。

## 许可

FreeCut 源码采用 [MIT](LICENSE)。依赖信息见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)，发布时附带生成的依赖许可清单。
