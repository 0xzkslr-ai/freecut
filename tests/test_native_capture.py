"""Real OS capture check: run on an isolated Xvfb display, never a user's desktop."""

import os

import pytest
import shiboken6
from PySide6.QtCore import QPoint, Qt
from PySide6.QtGui import QColor, QGuiApplication, QImage
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QWidget

from freecut.imaging import save_png
from freecut.selection import Mode
from freecut.window import MainWindow


@pytest.mark.skipif(
    os.environ.get("FREECUT_NATIVE_TEST") != "1", reason="Requires an isolated Xvfb display"
)
@pytest.mark.parametrize("mode", [Mode.LASSO, Mode.POLYGON])
@pytest.mark.parametrize("from_background", [False, True])
def test_real_screen_pixels_through_selection_to_png(
    qapp, tmp_path, mode, from_background, desktop_environment
):
    desktop_environment.supported = from_background
    desktop_environment.tray_available = from_background
    background = QWidget()
    background.setStyleSheet("background: #e56537;")
    background.setGeometry(QGuiApplication.primaryScreen().geometry())
    background.show()
    window = MainWindow(settings=desktop_environment.settings)
    window.show()
    try:
        window._mode_group.button(list(Mode).index(mode)).click()
        QTest.qWait(100)
        if from_background:
            window.hide_to_tray()
            window._hotkey.activated.emit()
        else:
            window.capture_button.click()
        QTest.qWait(30)
        overlay = window._overlay
        assert overlay is not None
        assert overlay.image.pixelColor(120, 120) == QColor("#e56537")
        if mode == Mode.POLYGON:
            for x, y in ((100, 100), (400, 100), (100, 350)):
                QTest.mouseClick(overlay, Qt.MouseButton.LeftButton, pos=QPoint(x, y))
            QTest.keyClick(overlay, Qt.Key.Key_Return)
        else:
            QTest.mousePress(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 100))
            QTest.mouseMove(overlay, QPoint(400, 100))
            QTest.mouseMove(overlay, QPoint(100, 350))
            QTest.mouseRelease(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 100))
        QTest.keyClick(overlay, Qt.Key.Key_Return)
        QTest.qWait(100)
        assert window.isVisible()
        assert window.image.width() == 300
        assert window.image.height() == 250
        assert window.image.pixelColor(5, 5) == QColor("#e56537")
        assert window.image.pixelColor(290, 240).alpha() == 0
        path = save_png(window.image, tmp_path / "native-screenshot.png")
        assert QImage(str(path)).pixelColor(290, 240).alpha() == 0
        window.copy_button.click()
        assert qapp.clipboard().mimeData().hasFormat("image/png")
        previous = window.image.copy()
        if from_background:
            window.hide_to_tray()
            window._hotkey.activated.emit()
        else:
            window.capture_button.click()
        QTest.qWait(30)
        QTest.keyClick(window._overlay, Qt.Key.Key_Escape)
        assert window.isVisible() == (not from_background)
        assert window.image == previous
    finally:
        window.shutdown()
        window.close()
        background.close()
        shiboken6.delete(window)
        shiboken6.delete(background)
