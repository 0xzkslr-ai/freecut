from unittest.mock import Mock

import pytest
import shiboken6
from PySide6.QtCore import QSettings, Qt
from PySide6.QtGui import QGuiApplication, QKeySequence, QPixmap
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QSystemTrayIcon

from freecut.hotkeys import DEFAULT_HOTKEY
from freecut.window import MainWindow


@pytest.fixture
def background_window(qapp, desktop_environment):
    desktop_environment.supported = True
    desktop_environment.tray_available = True
    window = MainWindow(settings=desktop_environment.settings)
    window.show()
    qapp.processEvents()
    yield window
    window.shutdown()
    window.close()
    shiboken6.delete(window)
    qapp.processEvents()


def test_tray_and_global_shortcut_are_ready(background_window):
    window = background_window
    assert window.background_available
    assert window._tray.isVisible()
    assert window._hotkey.sequence == DEFAULT_HOTKEY
    assert DEFAULT_HOTKEY in window.hotkey_status_label.text()
    assert window.hotkey_edit.isEnabled()
    assert DEFAULT_HOTKEY in window._tray_capture_action.text()
    assert "退出 FreeCut" in [action.text() for action in window._tray.contextMenu().actions()]


def test_close_hides_to_tray_and_keeps_registration(background_window):
    window = background_window
    window.close()
    assert not window.isVisible()
    assert not window._closing
    assert not window._hotkey.closed
    assert window._tray.isVisible()
    assert len(window._tray.messages) == 1
    assert DEFAULT_HOTKEY in window._tray.messages[0][1]
    window.show_main_window()
    window.close()
    assert len(window._tray.messages) == 1


@pytest.mark.parametrize(
    "reason",
    [QSystemTrayIcon.ActivationReason.Trigger, QSystemTrayIcon.ActivationReason.DoubleClick],
)
def test_clicking_tray_icon_restores_window(background_window, reason):
    window = background_window
    window.background_button.click()
    assert not window.isVisible()
    window._tray.activated.emit(reason)
    assert window.isVisible()
    window.hide_to_tray()
    window._tray.activated.emit(QSystemTrayIcon.ActivationReason.Context)
    assert not window.isVisible()
    window._tray.messageClicked.emit()
    assert window.isVisible()


def test_tray_show_menu_action(background_window):
    window = background_window
    window.hide_to_tray()
    action = next(
        action for action in window._tray.contextMenu().actions() if action.text() == "显示主窗口"
    )
    action.trigger()
    assert window.isVisible()


def test_changed_shortcut_is_saved_and_survives_new_window(
    background_window, desktop_environment, qapp
):
    window = background_window
    window.hotkey_edit.setKeySequence(QKeySequence("Ctrl+Shift+G"))
    window.hotkey_apply_button.click()
    assert window._hotkey.sequence == "Ctrl+Shift+G"
    assert desktop_environment.settings.value("capture/hotkey") == "Ctrl+Shift+G"
    assert "Ctrl+Shift+G" in window._tray.toolTip()
    window.shutdown()
    window.close()
    other = MainWindow(settings=desktop_environment.settings)
    try:
        assert other._hotkey.sequence == "Ctrl+Shift+G"
        assert other.hotkey_edit.keySequence().toString() == "Ctrl+Shift+G"
    finally:
        other.shutdown()
        other.close()
        shiboken6.delete(other)
        qapp.processEvents()


def test_conflicting_change_keeps_original_and_saved_settings(
    background_window, desktop_environment
):
    window = background_window
    window._apply_hotkey()
    desktop_environment.registration_error = "快捷键已被占用"
    window.hotkey_edit.setKeySequence(QKeySequence("Ctrl+Alt+B"))
    window.hotkey_apply_button.click()
    assert window._hotkey.sequence == DEFAULT_HOTKEY
    assert "仍有效" in window.hotkey_status_label.text()
    assert desktop_environment.settings.value("capture/hotkey") == DEFAULT_HOTKEY
    assert DEFAULT_HOTKEY in window._tray_capture_action.text()


def test_empty_shortcut_does_not_remove_working_binding(background_window):
    window = background_window
    window.hotkey_edit.clear()
    window.hotkey_apply_button.click()
    assert window._hotkey.sequence == DEFAULT_HOTKEY
    assert "快捷键" in window.hotkey_status_label.text()


def test_corrupt_saved_shortcut_uses_safe_default(qapp, desktop_environment):
    desktop_environment.supported = True
    desktop_environment.settings.setValue("capture/hotkey", "not a shortcut")
    window = MainWindow(settings=desktop_environment.settings)
    try:
        assert window._hotkey.sequence == DEFAULT_HOTKEY
    finally:
        window.shutdown()
        window.close()
        shiboken6.delete(window)


def test_startup_hotkey_conflict_is_visible_without_preventing_capture(qapp, desktop_environment):
    desktop_environment.supported = True
    desktop_environment.tray_available = True
    desktop_environment.registration_error = "快捷键已被其他应用占用"
    window = MainWindow(settings=desktop_environment.settings)
    try:
        assert not window._hotkey.sequence
        assert "占用" in window.hotkey_status_label.text()
        assert window.capture_button.isEnabled()
        assert window.hide_to_tray()
        assert "右键" in window._tray.messages[0][1]
    finally:
        window.shutdown()
        window.close()
        shiboken6.delete(window)


def test_setting_write_failure_reports_active_shortcut(background_window, monkeypatch):
    window = background_window
    monkeypatch.setattr(window._settings, "status", lambda: QSettings.Status.AccessError)
    window._apply_hotkey()
    assert window._hotkey.sequence == DEFAULT_HOTKEY
    assert "已生效" in window.hotkey_status_label.text()
    assert "保存失败" in window.hotkey_status_label.text()


def test_hotkey_captures_from_background_and_ignores_repeated_requests(background_window, qapp):
    window = background_window
    window.hide_to_tray()
    window._hotkey.activated.emit()
    qapp.processEvents()
    assert window._capturing
    assert not window.isVisible()
    assert window._return_to_background
    timer_id = window._capture_timer.timerId()
    window._hotkey.activated.emit()
    qapp.processEvents()
    assert window._capture_timer.timerId() == timer_id
    window._capture_timer.stop()
    window._on_cancel()
    assert not window.isVisible()
    assert not window._capturing
    window.show_main_window()
    assert window.isVisible()


def test_background_cancellation_stays_hidden_and_keeps_last_result(
    background_window, image, monkeypatch
):
    window = background_window
    window._on_capture(image)
    window.hide_to_tray()
    screen = QGuiApplication.screens()[0]
    monkeypatch.setattr(screen, "grabWindow", lambda *args: QPixmap.fromImage(image))
    window._capture_from_hotkey()
    window._capture_timer.stop()
    window._take_capture()
    assert window._overlay is not None
    QTest.keyClick(window._overlay, Qt.Key.Key_Escape)
    assert not window.isVisible()
    assert window.image == image
    assert not window._capturing


def test_background_capture_result_opens_preview(background_window, image, monkeypatch):
    window = background_window
    screen = QGuiApplication.screens()[0]
    monkeypatch.setattr(screen, "grabWindow", lambda *args: QPixmap.fromImage(image))
    window.hide_to_tray()
    window._capture_from_hotkey()
    window._capture_timer.stop()
    window._take_capture()
    overlay = window._overlay
    overlay.selection.points = [(40, 40), (300, 40), (40, 300)]
    overlay.selection.finish()
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    assert window.isVisible()
    assert not window.image.isNull()


def test_background_capture_error_reopens_window(background_window, monkeypatch):
    from freecut.window import QMessageBox

    window = background_window
    window.hide_to_tray()
    window._capture_from_hotkey()
    window._capture_timer.stop()
    monkeypatch.setattr(window._target_screen, "grabWindow", lambda *args: QPixmap())
    monkeypatch.setattr(QMessageBox, "warning", Mock())
    window._take_capture()
    assert window.isVisible()
    assert not window._capturing


def test_modal_dialog_blocks_hotkey(background_window, monkeypatch):
    window = background_window
    monkeypatch.setattr(QApplication, "activeModalWidget", lambda: object())
    window._capture_from_hotkey()
    assert not window._capturing


def test_cannot_hide_or_reopen_main_while_selecting(background_window):
    window = background_window
    window._begin_capture()
    assert not window.hide_to_tray()
    window.show_main_window()
    assert not window.isVisible()
    window._capture_timer.stop()


def test_tray_disappearing_does_not_trap_window(
    background_window, desktop_environment, qapp, monkeypatch
):
    window = background_window
    quit_app = Mock()
    monkeypatch.setattr(qapp, "quit", quit_app)
    desktop_environment.tray_available = False
    window.close()
    assert window._closing
    assert window._hotkey.closed
    assert not window.isVisible()
    quit_app.assert_called_once()


def test_tray_disappearing_during_capture_restores_window(background_window, desktop_environment):
    window = background_window
    window.hide_to_tray()
    window._capture_from_hotkey()
    window._capture_timer.stop()
    desktop_environment.tray_available = False
    window._on_cancel()
    assert window.isVisible()


def test_quit_action_releases_hotkey_stops_timer_and_exits(background_window, qapp, monkeypatch):
    window = background_window
    quit_app = Mock()
    monkeypatch.setattr(qapp, "quit", quit_app)
    window._begin_capture()
    action = next(
        action for action in window._tray.contextMenu().actions() if action.text() == "退出 FreeCut"
    )
    action.trigger()
    assert window._closing
    assert window._hotkey.closed
    assert not window._capture_timer.isActive()
    assert not window._tray.isVisible()
    assert not window.isVisible()
    quit_app.assert_called_once()
    assert not window.hide_to_tray()
    window.show_main_window()
    window._capture_from_hotkey()
    assert not window.isVisible()
    assert not window._capture_timer.isActive()


def test_hotkey_targets_pointer_monitor(background_window, monkeypatch):
    window = background_window
    screen = window.screen_combo.itemData(0)
    window.screen_combo.addItem("另一屏幕", None)
    window.screen_combo.setCurrentIndex(1)
    monkeypatch.setattr(QGuiApplication, "screenAt", lambda position: screen)
    window._capture_from_hotkey()
    assert window.screen_combo.currentIndex() == 0
    assert window._target_screen is screen
    window._capture_timer.stop()


def test_no_tray_available_keeps_normal_exit_behavior(qapp, desktop_environment):
    window = MainWindow(settings=desktop_environment.settings)
    try:
        window.show()
        assert not window.background_available
        assert not window.background_button.isEnabled()
        assert not window.hotkey_edit.isEnabled()
        assert not window.hide_to_tray()
        window.close()
        assert window._closing
    finally:
        window.shutdown()
        shiboken6.delete(window)
