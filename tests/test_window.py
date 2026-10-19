from unittest.mock import Mock

import pytest
import shiboken6
from PySide6.QtCore import Qt
from PySide6.QtGui import QGuiApplication, QImage, QPixmap
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QMessageBox

from freecut import window as window_module
from freecut.selection import Mode
from freecut.window import MainWindow


@pytest.fixture
def window(qapp, desktop_environment):
    widget = MainWindow(settings=desktop_environment.settings)
    widget.show()
    qapp.processEvents()
    yield widget
    widget.shutdown()
    widget.close()
    shiboken6.delete(widget)
    qapp.processEvents()


def test_initial_state_and_mode_buttons(window):
    assert window.image.isNull()
    assert not window.copy_button.isEnabled()
    assert not window.save_button.isEnabled()
    assert window.screen_combo.count() >= 1
    window._mode_group.button(1).click()
    assert window.mode == Mode.POLYGON
    window._mode_group.button(0).click()
    assert window.mode == Mode.LASSO
    assert not window.grab().isNull()


def test_capture_preview_and_native_clipboard(window, image):
    window._on_capture(image)
    assert window.preview.image == image
    assert window.copy_button.isEnabled()
    assert window.save_button.isEnabled()
    assert "160 × 120" in window.dimensions_label.text()
    window.copy_button.click()
    assert QApplication.clipboard().mimeData().hasFormat("image/png")
    assert "已复制" in window.status_label.text()
    assert not window.grab().isNull()


def test_no_result_save_and_copy_are_noops(window, monkeypatch):
    chooser = Mock()
    monkeypatch.setattr(window_module.QFileDialog, "getSaveFileName", chooser)
    window._save()
    window._copy()
    chooser.assert_not_called()


def test_save_png(window, image, tmp_path, monkeypatch):
    window._on_capture(image)
    path = tmp_path / "保存截图.png"
    monkeypatch.setattr(
        window_module.QFileDialog, "getSaveFileName", lambda *args: (str(path), "PNG")
    )
    window.save_button.click()
    assert QImage(str(path)).size() == image.size()
    assert "已保存" in window.status_label.text()


def test_cancel_save(window, image, monkeypatch):
    window._on_capture(image)
    monkeypatch.setattr(window_module.QFileDialog, "getSaveFileName", lambda *args: ("", ""))
    window._save()
    assert "截图完成" in window.status_label.text()


@pytest.mark.parametrize("replace", [False, True])
def test_png_suffix_collision_requires_confirmation(window, image, tmp_path, monkeypatch, replace):
    window._on_capture(image)
    path = tmp_path / "result.png"
    path.write_bytes(b"original")
    monkeypatch.setattr(
        window_module.QFileDialog,
        "getSaveFileName",
        lambda *args: (str(tmp_path / "result"), "PNG"),
    )
    monkeypatch.setattr(
        QMessageBox,
        "question",
        lambda *args: QMessageBox.StandardButton.Yes if replace else QMessageBox.StandardButton.No,
    )
    window._save()
    assert (path.read_bytes() != b"original") == replace


def test_save_error_shown_without_crash(window, image, tmp_path, monkeypatch):
    window._on_capture(image)
    monkeypatch.setattr(
        window_module.QFileDialog,
        "getSaveFileName",
        lambda *args: (str(tmp_path / "missing" / "test.png"), "PNG"),
    )
    warning = Mock()
    monkeypatch.setattr(QMessageBox, "warning", warning)
    window._save()
    warning.assert_called_once()
    assert "无法保存" in window.status_label.text()


def test_clipboard_error_recommends_file(window, image, monkeypatch):
    window._on_capture(image)
    monkeypatch.setattr(window_module, "image_mime_data", Mock(side_effect=OSError("failed")))
    monkeypatch.setattr(QMessageBox, "warning", Mock())
    window._copy()
    assert "保存 PNG" in window.status_label.text()


def test_capture_start_hides_window_and_debounces(window):
    window._begin_capture()
    assert not window.isVisible()
    assert window._capturing
    assert not window.capture_button.isEnabled()
    assert window._capture_timer.isActive()
    target = window._target_screen
    window._begin_capture()
    assert window._target_screen is target
    window._capture_timer.stop()
    window._on_cancel()
    assert window.isVisible()
    assert window.capture_button.isEnabled()
    assert "取消" in window.status_label.text()


def test_full_capture_workflow_with_mocked_os_pixels(window, image, monkeypatch, qapp):
    screen = QGuiApplication.screens()[0]
    monkeypatch.setattr(screen, "grabWindow", lambda *args: QPixmap.fromImage(image))
    window._begin_capture()
    window._capture_timer.stop()
    window._take_capture()
    overlay = window._overlay
    assert overlay is not None
    assert overlay.isVisible()
    overlay.selection.points = [(40, 40), (400, 40), (40, 300)]
    overlay.selection.finish()
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    assert not window.image.isNull()
    assert window.isVisible()
    assert not window._capturing
    qapp.processEvents()


def test_overlay_cancel_restores_window(window, image, monkeypatch):
    screen = QGuiApplication.screens()[0]
    monkeypatch.setattr(screen, "grabWindow", lambda *args: QPixmap.fromImage(image))
    window._begin_capture()
    window._capture_timer.stop()
    window._take_capture()
    QTest.keyClick(window._overlay, Qt.Key.Key_Escape)
    assert window.isVisible()
    assert not window._capturing


@pytest.mark.parametrize("disconnected", [False, True])
def test_failed_capture_restores_window(window, monkeypatch, disconnected):
    warning = Mock()
    monkeypatch.setattr(QMessageBox, "warning", warning)
    window._begin_capture()
    window._capture_timer.stop()
    if disconnected:
        window._target_screen = None
    else:
        monkeypatch.setattr(window._target_screen, "grabWindow", lambda *args: QPixmap())
    window._take_capture()
    assert window.isVisible()
    assert not window._capturing
    assert window.capture_button.isEnabled()
    warning.assert_called_once()


def test_missing_screen_shows_error(window, monkeypatch):
    window.screen_combo.clear()
    warning = Mock()
    monkeypatch.setattr(QMessageBox, "warning", warning)
    window._begin_capture()
    assert not window._capturing
    warning.assert_called_once()


def test_screen_refresh_preserves_choice(window):
    previous = window.screen_combo.currentData()
    window._refresh_screens()
    assert window.screen_combo.currentData() is previous


def test_close_cancels_pending_capture(window):
    window._begin_capture()
    window.close()
    assert not window._capture_timer.isActive()
    window._begin_capture()
    assert not window._capture_timer.isActive()


def test_close_with_overlay_does_not_reopen_main_window(window, image, monkeypatch):
    screen = QGuiApplication.screens()[0]
    monkeypatch.setattr(screen, "grabWindow", lambda *args: QPixmap.fromImage(image))
    window._begin_capture()
    window._capture_timer.stop()
    window._take_capture()
    window.close()
    assert not window.isVisible()
    assert window._overlay is None
