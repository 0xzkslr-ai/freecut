# Third-party notices

FreeCut's own source code is MIT licensed. Dependencies retain their own licenses.

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
