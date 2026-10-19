import os
from types import SimpleNamespace

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest  # noqa: E402
from PySide6.QtCore import QEvent, QObject, QSettings, Signal  # noqa: E402
from PySide6.QtGui import QColor, QImage  # noqa: E402
from PySide6.QtWidgets import QApplication, QSystemTrayIcon  # noqa: E402

from freecut import window as window_module  # noqa: E402
from freecut.hotkeys import parse_hotkey  # noqa: E402
from freecut.theme import STYLESHEET  # noqa: E402


@pytest.fixture(scope="session")
def qapp():
    app = QApplication.instance() or QApplication([])
    app.setQuitOnLastWindowClosed(False)
    app.setStyle("Fusion")
    app.setStyleSheet(STYLESHEET)
    yield app
    # Qt's offscreen clipboard owns MIME data until explicitly cleared; release
    # it before Python/Qt interpreter teardown and discard deferred widget deletes.
    app.clipboard().clear()
    app.closeAllWindows()
    app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
    app.processEvents()


@pytest.fixture
def image():
    result = QImage(160, 120, QImage.Format.Format_ARGB32_Premultiplied)
    result.fill(QColor("#e56537"))
    return result


@pytest.fixture
def desktop_environment(monkeypatch, tmp_path):
    """Mock OS integrations; tests never reserve a developer's real hotkeys/tray."""
    environment = SimpleNamespace(
        supported=False,
        tray_available=False,
        registration_error="",
        hotkeys=[],
        trays=[],
        settings=QSettings(str(tmp_path / "settings.ini"), QSettings.Format.IniFormat),
    )

    class FakeHotkey(QObject):
        activated = Signal()

        def __init__(self, parent):
            super().__init__(parent)
            self.supported = environment.supported
            self.sequence = ""
            self.closed = False
            environment.hotkeys.append(self)

        def register(self, sequence):
            spec = parse_hotkey(sequence)
            if environment.registration_error:
                raise OSError(environment.registration_error)
            self.sequence = spec.text
            return self.sequence

        def close(self):
            self.closed = True
            self.sequence = ""

    class FakeTray(QObject):
        activated = Signal(object)
        messageClicked = Signal()
        ActivationReason = QSystemTrayIcon.ActivationReason

        def __init__(self, icon, parent):
            super().__init__(parent)
            self._visible = False
            self._menu = None
            self._tooltip = ""
            self.messages = []
            environment.trays.append(self)

        @staticmethod
        def isSystemTrayAvailable():
            return environment.tray_available

        def setContextMenu(self, menu):
            self._menu = menu

        def contextMenu(self):
            return self._menu

        def setToolTip(self, tooltip):
            self._tooltip = tooltip

        def toolTip(self):
            return self._tooltip

        def show(self):
            self._visible = True

        def hide(self):
            self._visible = False

        def isVisible(self):
            return self._visible

        def showMessage(self, title, message):
            self.messages.append((title, message))

    monkeypatch.setattr(window_module, "GlobalHotkey", FakeHotkey)
    monkeypatch.setattr(window_module, "QSystemTrayIcon", FakeTray)
    return environment
