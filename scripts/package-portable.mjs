import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const directory = path.join(root, "dist/FreeCut-Tauri");
const executable = process.argv[2] || "src-tauri/target/release/FreeCut.exe";
const files = [
  [executable, "FreeCut.exe"],
  ["LICENSE", "LICENSE.txt"],
  ["THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"],
  ["src-tauri/THIRD_PARTY_LICENSES.txt", "THIRD_PARTY_LICENSES.txt"],
];
for (const [source] of files) fs.accessSync(path.join(root, source));
fs.mkdirSync(directory, { recursive: true });
for (const [source, target] of files)
  fs.copyFileSync(path.join(root, source), path.join(directory, target));
fs.writeFileSync(
  path.join(directory, "使用说明.txt"),
  "FreeCut · 自由形状截图\n\n运行 FreeCut.exe。默认全局截图键 Ctrl+Alt+A，可在窗口中修改。\n套索松开闭合；多边形点击起点 / 双击 / Enter 闭合；矩形拖拽框选、松开完成，再按 Enter 确认。\nL / P / M 切换套索 / 多边形 / 矩形。矩形四个角点可拖动缩放。\nEsc 取消，Tab 隐藏提示。关闭窗口后驻留托盘，右键托盘可退出。\n\n可使用 FreeCut.exe --background 直接启动到托盘。\n本程序使用系统 WebView2 Runtime。缺少运行时时，请使用安装包或先安装 Microsoft WebView2。\n",
);
const size = fs
  .readdirSync(directory)
  .reduce(
    (total, name) => total + fs.statSync(path.join(directory, name)).size,
    0,
  );
console.log(
  `Portable directory: ${directory}\nTotal: ${(size / 1e6).toFixed(2)} MB`,
);
