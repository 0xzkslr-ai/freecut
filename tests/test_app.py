import os
import subprocess
import sys
from unittest.mock import Mock

import pytest

from freecut import app as app_module
from freecut.app import app_icon
from freecut.preview import Preview


def test_application_icon(qapp):
    assert not app_icon().isNull()


def test_app_entry_point(monkeypatch):
    application = Mock()
    application.exec.return_value = 0
    window = Mock()
    monkeypatch.setattr(app_module, "QApplication", lambda args: application)
    monkeypatch.setattr(app_module, "MainWindow", lambda: window)
    monkeypatch.setattr(app_module, "app_icon", lambda: Mock())
    assert app_module.main([]) == 0
    application.setApplicationName.assert_called_once_with("FreeCut")
    window.show.assert_called_once()
    application.exec.assert_called_once()
    window.deleteLater.assert_called_once()
    application.sendPostedEvents.assert_called_once()


@pytest.mark.parametrize("tray_available", [True, False])
def test_background_startup_respects_tray_availability(monkeypatch, tray_available):
    application = Mock()
    application.exec.return_value = 0
    window = Mock()
    window.background_available = tray_available
    monkeypatch.setattr(app_module, "QApplication", lambda args: application)
    monkeypatch.setattr(app_module, "MainWindow", lambda: window)
    monkeypatch.setattr(app_module, "app_icon", lambda: Mock())
    assert app_module.main(["--background"]) == 0
    if tray_available:
        window.hide_to_tray.assert_called_once()
        window.show.assert_not_called()
    else:
        window.show.assert_called_once()
        window.status_label.setText.assert_called_once()
    window.shutdown.assert_called_once()


def test_real_app_starts_and_exits_without_teardown_crash():
    script = """
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QSystemTrayIcon
from freecut import app
from freecut import hotkeys

hotkeys._load_user32 = lambda: None
QSystemTrayIcon.isSystemTrayAvailable = staticmethod(lambda: False)

OriginalWindow = app.MainWindow

class AutoCloseWindow(OriginalWindow):
    def __init__(self):
        super().__init__()
        QTimer.singleShot(80, self.close)

app.MainWindow = AutoCloseWindow
raise SystemExit(app.main())
"""
    result = subprocess.run(
        [sys.executable, "-X", "faulthandler", "-c", script],
        env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert result.returncode == 0, result.stderr


def test_preview_empty_and_populated(qapp, image):
    preview = Preview()
    preview.resize(400, 300)
    preview.show()
    assert not preview.grab().isNull()
    preview.set_image(image)
    assert preview.image == image
    image.fill(0)
    assert preview.image != image
    assert not preview.grab().isNull()
    preview.close()
