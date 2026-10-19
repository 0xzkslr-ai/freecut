# FreeCut

自由形状截图工具。用套索画出轮廓，或用多边形逐点勾勒，再保存为透明 PNG。
Python + PySide6 桌面应用，所有图像处理均在本机完成。

## 当前功能

- **自由套索**：按住鼠标左键绘制，松开后自动闭合。
- **多边形**：逐点添加顶点，点击起点、双击或按 Enter 闭合。
- 确认前拖动选区移动位置，多边形可拖动顶点调整形状。
- 多边形撤销顶点，重选和取消；隐藏操作提示后可选择提示栏下方内容。
- 透明棋盘格预览；透明 PNG 导出；复制原生图片和 PNG 到剪贴板。
- 按显示器选择截屏，支持延迟截图；按截图实际像素导出，适配缩放。
- 中文界面，离线使用；不会自动保存、上传截图，也不申请管理员权限。

当前不包含智能选取、模型下载、OCR、托盘常驻或全局热键。

## Windows 启动

安装 **Python 3.10 或更新版本**，然后双击项目根目录的 **`run.bat`**。
首次运行会创建 `.venv` 并下载依赖，需要联网；安装完成后使用不需要联网。

也可以在 PowerShell 中执行：

```powershell
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -e .
.\.venv\Scripts\python.exe -m freecut
```

请使用 **Windows 原生 Python** 截取 Windows 桌面。WSL 的 Linux Python 不能代替
Windows 原生截图后端。项目也可在有图形会话的 Linux/X11 环境运行；Wayland
截图权限 / portal 接入和 macOS 的截图授权尚未完成适配和真机验证。

## 使用

1. 选择「自由套索」或「多边形」、目标显示器和截图延迟。
2. 点击「开始截图」。主窗口会暂时隐藏，随后冻结目标屏幕画面。
3. 绘制选区。套索松开即闭合；多边形可以逐点选取凹多边形。
4. 闭合后可拖动选区内部移动，多边形顶点可以单独调整。
5. 按 Enter 或点击「确认」，返回主窗口查看透明效果。
6. 复制图片，或保存透明 PNG。

| 场景 | 快捷键 | 操作 |
| --- | --- | --- |
| 主窗口内 | Ctrl+Shift+A | 开始截图 |
| 主窗口内 | Ctrl+S / Ctrl+C | 保存 / 复制已有截图 |
| 圈选时 | L / P | 切换套索 / 多边形，并清空当前选区 |
| 多边形绘制中 | 右键 / Backspace / Delete / Ctrl+Z | 撤销一个顶点 |
| 多边形闭合后 | 右键 / Backspace / Ctrl+Z | 重新打开轮廓，继续编辑 |
| 圈选时 | Enter | 闭合未完成的多边形；已闭合时确认截图 |
| 圈选时 | R / Esc | 重新选择 / 取消本次截图 |
| 圈选时 | Tab | 隐藏 / 显示提示栏和确认工具条 |

PNG 画布仍为选区的最小外接矩形，轮廓外的像素透明；预览棋盘格不会导出。
边缘使用抗锯齿蒙版，不额外绘制轮廓线，也不缩小原始像素。
自交轮廓采用奇偶填充规则。复制后的透明效果取决于目标软件，PNG 文件保留 alpha。

第一版一次选择一个显示器，不支持跨多个显示器绘制同一选区。显示器尺寸使用
Qt 的逻辑坐标，导出使用截图的物理像素；不同 DPI 的实际多屏组合仍需要真机验证。

## 开发与测试

```sh
python -m pip install -e ".[dev]"
ruff check .
ruff format --check .
python -m pytest
python -m build
```

无显示器的测试环境设置 `QT_QPA_PLATFORM=offscreen`。测试覆盖选区状态、撤销、移动
边界、透明与抗锯齿像素、DPI 坐标转换、PNG 读写、剪贴板数据、鼠标 / 键盘操作、
主窗口截图流程和错误恢复。CI 配置同时运行 Windows / Linux 的离屏测试。

Linux 下还可以在独立的 Xvfb 桌面中检查真实屏幕像素，验证两种圈选到 PNG、复制、
取消的完整流程（不会截取使用者桌面）：

```sh
xvfb-run -a env QT_QPA_PLATFORM=xcb FREECUT_NATIVE_TEST=1 python -m pytest -o addopts= tests/test_native_capture.py
```

X11 测试需要 `xvfb`、`xauth` 以及 Qt xcb 插件依赖；Ubuntu 可安装：

```sh
sudo apt-get install xvfb xauth libegl1 libopengl0 libxcb-cursor0 libxkbcommon-x11-0 libxcb-icccm4 libxcb-keysyms1 libxcb-xkb1
```

```text
src/freecut/
  app.py        应用入口与图标
  window.py     截图调度、屏幕选择、复制与保存
  overlay.py    屏幕选区与交互
  selection.py  选区状态和坐标运算
  imaging.py    透明蒙版、PNG 与剪贴板数据
  preview.py    透明背景预览
  theme.py      共享界面样式
```

## Windows 打包

先运行 `run.bat` 安装环境，再双击 `build_windows.bat`。
输出目录为 `dist\FreeCut\`，入口是 `FreeCut.exe`；分发时保留整个目录。
Windows 程序需要在 Windows 上构建；Linux 构建不能生成 Windows 可执行文件。

## 开源许可

FreeCut 源码采用 [MIT](LICENSE) 许可，依赖保留各自许可。
Qt / PySide6 的许可、源码获取与二进制分发说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
