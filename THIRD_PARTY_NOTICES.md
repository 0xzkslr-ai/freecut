# Third-party notices

FreeCut's own source code is MIT licensed. Dependencies retain their own licenses.

## Dependencies

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
