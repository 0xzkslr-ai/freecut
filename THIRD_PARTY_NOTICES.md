# Third-party notices

FreeCut's own source code is MIT licensed. Dependencies retain their own licenses.

## Tauri edition (0.2 and later)

- **Tauri 2 / @tauri-apps/api / global-shortcut / single-instance**, Tauri
  contributors: MIT OR Apache-2.0. https://github.com/tauri-apps/tauri
- **tiny-skia**, contributors: BSD-3-Clause. https://github.com/linebender/tiny-skia
- **png**, image-rs contributors: MIT OR Apache-2.0. https://github.com/image-rs/image-png
- **serde / serde_json**, contributors: MIT OR Apache-2.0. https://serde.rs
- **windows-sys**, Microsoft and contributors: MIT OR Apache-2.0.
  https://github.com/microsoft/windows-rs
- **rfd**, contributors: MIT. https://github.com/PolyMeilex/rfd
- **tempfile**, contributors: MIT OR Apache-2.0. https://github.com/Stebalien/tempfile

Run `node scripts/licenses.mjs` before packaging to regenerate the locked Windows
dependency inventory and full license texts at `src-tauri/THIRD_PARTY_LICENSES.txt`.
The release script includes that file and this notice with portable distributions;
the Tauri installer includes the generated license file and FreeCut's MIT license.
Build dependencies are included in the inventory as well as runtime dependencies.

Windows uses the system **Microsoft Edge WebView2 Runtime**, distributed separately
under Microsoft's terms. It is not statically included in FreeCut. The installer
can download the official bootstrapper when the runtime is missing.

No model weights, external fonts or remote inference services are included.
The icon is FreeCut's existing design, reproduced from `src-tauri/icons/source.svg`.

## Python edition (retained source / old packages only)

- **Qt / PySide6 / Shiboken6**, The Qt Company and contributors. The Qt for Python
  Essentials modules used here are available under LGPLv3 / GPLv3 / commercial
  terms. FreeCut uses the LGPL option and dynamically loaded, unmodified libraries.
  See the [Qt for Python licensing documentation](https://doc.qt.io/qtforpython-6/licenses.html),
  [LGPLv3 text](https://www.gnu.org/licenses/lgpl-3.0.html),
  [GPLv3 text](https://www.gnu.org/licenses/gpl-3.0.html), and
  [matching Qt for Python source distributions](https://download.qt.io/official_releases/QtForPython/).
  Installed wheels include their license texts under the package metadata / Qt
  license directories; retain these when redistributing a binary build.

The Windows build script creates an **onedir** bundle so that shared Qt libraries
remain separate. When publishing a binary, include the dependency license texts,
the exact dependency versions, and corresponding-source information; verify that
the bundled libraries can be replaced with compatible modified versions. This
notice is a dependency inventory, not a claim that an unreviewed binary release
has satisfied every redistribution obligation.

No model weights, remote inference services, external fonts, or image assets are
included. The application icon is drawn by FreeCut itself.
