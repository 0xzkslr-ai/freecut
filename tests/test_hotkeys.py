import ctypes
from ctypes import wintypes
from types import SimpleNamespace
from unittest.mock import Mock

import pytest
from PySide6.QtCore import QByteArray
from PySide6.QtGui import QKeySequence

from freecut import hotkeys
from freecut.hotkeys import DEFAULT_HOTKEY, MOD_NOREPEAT, WM_HOTKEY, GlobalHotkey, parse_hotkey


@pytest.mark.parametrize(
    ("text", "modifiers", "virtual_key"),
    [
        (DEFAULT_HOTKEY, 3, ord("A")),
        ("Ctrl+Shift+2", 6, ord("2")),
        ("Alt+F1", 1, 0x70),
        ("Ctrl+Alt+Shift+F24", 7, 0x87),
    ],
)
def test_parse_supported_chords(text, modifiers, virtual_key):
    spec = parse_hotkey(QKeySequence(text))
    assert spec.text == text
    assert spec.modifiers == modifiers
    assert spec.virtual_key == virtual_key


@pytest.mark.parametrize(
    "text", ["", "A", "Shift+A", "Meta+A", "Ctrl+F12", "Ctrl+Space", "Ctrl+A, Ctrl+B", "Ctrl"]
)
def test_invalid_or_reserved_chords(text):
    with pytest.raises(ValueError):
        parse_hotkey(text)


def test_windows_api_loader_declares_ffi_types(monkeypatch):
    api = Mock()
    loader = Mock(return_value=api)
    monkeypatch.setattr(hotkeys, "sys", SimpleNamespace(platform="win32"))
    monkeypatch.setattr(hotkeys.ctypes, "WinDLL", loader, raising=False)
    assert hotkeys._load_user32() is api
    loader.assert_called_once_with("user32", use_last_error=True)
    assert api.RegisterHotKey.argtypes == [
        wintypes.HWND,
        ctypes.c_int,
        wintypes.UINT,
        wintypes.UINT,
    ]
    assert api.UnregisterHotKey.argtypes == [wintypes.HWND, ctypes.c_int]


def test_non_windows_backend_is_explicitly_unsupported(qapp, monkeypatch):
    monkeypatch.setattr(hotkeys, "sys", SimpleNamespace(platform="linux"))
    service = GlobalHotkey()
    assert not service.supported
    with pytest.raises(OSError, match="Windows"):
        service.register(DEFAULT_HOTKEY)
    service.close()


def test_registration_no_repeat_and_idempotent_release(qapp):
    api = Mock()
    api.RegisterHotKey.return_value = True
    service = GlobalHotkey(user32=api)
    try:
        assert service.supported
        assert service.register(DEFAULT_HOTKEY) == DEFAULT_HOTKEY
        assert service.sequence == DEFAULT_HOTKEY
        assert api.RegisterHotKey.call_args.args[2:] == (3 | MOD_NOREPEAT, ord("A"))
        service.register(DEFAULT_HOTKEY)
        api.RegisterHotKey.assert_called_once()
        service.close()
        service.close()
        api.UnregisterHotKey.assert_called_once()
        assert service.sequence == ""
        assert service._filter.active_id is None
        assert not service._installed
    finally:
        service.close()


def test_hotkey_change_acquires_new_chord_before_releasing_old(qapp):
    api = Mock()
    api.RegisterHotKey.return_value = True
    service = GlobalHotkey(user32=api)
    try:
        service.register(DEFAULT_HOTKEY)
        first_id = service._filter.active_id
        api.reset_mock()
        service.register("Ctrl+Shift+S")
        assert service._filter.active_id != first_id
        assert api.method_calls[0][0] == "RegisterHotKey"
        assert api.method_calls[1] == ("UnregisterHotKey", (None, first_id), {})
        second_id = service._filter.active_id
        service.register("Ctrl+Alt+G")
        assert service._filter.active_id == first_id
        assert second_id != first_id
    finally:
        service.close()


def test_conflict_keeps_old_shortcut_active_and_does_not_leak_registration(qapp):
    api = Mock()
    api.RegisterHotKey.return_value = True
    service = GlobalHotkey(user32=api)
    try:
        service.register(DEFAULT_HOTKEY)
        first_id = service._filter.active_id
        api.RegisterHotKey.return_value = False
        with pytest.raises(OSError, match="占用"):
            service.register("Ctrl+Alt+B")
        assert service.sequence == DEFAULT_HOTKEY
        assert service._filter.active_id == first_id
        api.UnregisterHotKey.assert_not_called()
    finally:
        service.close()


def test_initial_conflict_does_not_install_filter(qapp):
    api = Mock()
    api.RegisterHotKey.return_value = False
    service = GlobalHotkey(user32=api)
    with pytest.raises(OSError):
        service.register(DEFAULT_HOTKEY)
    assert not service._installed
    service.close()
    api.UnregisterHotKey.assert_not_called()


@pytest.mark.parametrize("event_type", [b"windows_generic_MSG", b"windows_dispatcher_MSG"])
def test_native_windows_message_activates_only_registered_shortcut(qapp, event_type):
    api = Mock()
    api.RegisterHotKey.return_value = True
    service = GlobalHotkey(user32=api)
    called = []
    service.activated.connect(lambda: called.append(True))
    try:
        service.register(DEFAULT_HOTKEY)
        message = wintypes.MSG()
        message.message = WM_HOTKEY
        message.wParam = service._filter.active_id
        pointer = ctypes.addressof(message)
        assert service._filter.nativeEventFilter(QByteArray(event_type), pointer) == (True, 0)
        assert called == [True]
        message.wParam += 100
        assert service._filter.nativeEventFilter(event_type, pointer) == (False, 0)
        message.wParam = service._filter.active_id
        message.message = 0
        assert service._filter.nativeEventFilter(event_type, pointer) == (False, 0)
        assert service._filter.nativeEventFilter(b"xcb_generic_event_t", 1) == (False, 0)
        assert service._filter.nativeEventFilter(event_type, 0) == (False, 0)
        service.close()
        assert service._filter.nativeEventFilter(event_type, pointer) == (False, 0)
        assert called == [True]
    finally:
        service.close()


def test_registration_requires_live_application(monkeypatch):
    api = Mock()
    service = GlobalHotkey(user32=api)
    monkeypatch.setattr(hotkeys, "QCoreApplication", SimpleNamespace(instance=lambda: None))
    with pytest.raises(RuntimeError, match="Qt"):
        service.register(DEFAULT_HOTKEY)
    api.RegisterHotKey.assert_not_called()


def test_filter_cleanup_tolerates_missing_application(qapp, monkeypatch):
    api = Mock()
    service = GlobalHotkey(user32=api)
    service.register(DEFAULT_HOTKEY)
    # Remove the real test filter before simulating teardown of QApplication.
    qapp.removeNativeEventFilter(service._filter)
    monkeypatch.setattr(hotkeys, "QCoreApplication", SimpleNamespace(instance=lambda: None))
    service.close()
    assert not service._installed
