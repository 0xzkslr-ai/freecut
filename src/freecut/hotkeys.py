"""Windows global hotkeys without a keyboard hook, polling, or elevated permissions."""

import ctypes
import sys
from collections.abc import Callable
from ctypes import wintypes
from dataclasses import dataclass

from PySide6.QtCore import QAbstractNativeEventFilter, QCoreApplication, QObject, Qt, Signal
from PySide6.QtGui import QKeySequence

DEFAULT_HOTKEY = "Ctrl+Alt+A"
WM_HOTKEY = 0x0312
MOD_NOREPEAT = 0x4000
_HOTKEY_IDS = (0x4643, 0x4644)


@dataclass(frozen=True)
class HotkeySpec:
    text: str
    modifiers: int
    virtual_key: int


def parse_hotkey(sequence: str | QKeySequence) -> HotkeySpec:
    """Translate one Ctrl/Alt chord to Win32 modifiers and a virtual-key code."""
    sequence = QKeySequence(sequence)
    if sequence.count() != 1 or sequence.isEmpty():
        raise ValueError("请设置一组快捷键，例如 Ctrl+Alt+A。")
    combination = sequence[0]
    modifiers = combination.keyboardModifiers()
    allowed = (
        Qt.KeyboardModifier.ControlModifier
        | Qt.KeyboardModifier.AltModifier
        | Qt.KeyboardModifier.ShiftModifier
    )
    if modifiers & ~allowed or not modifiers & (
        Qt.KeyboardModifier.ControlModifier | Qt.KeyboardModifier.AltModifier
    ):
        raise ValueError("快捷键需包含 Ctrl 或 Alt，可搭配 Shift；不支持 Windows 键。")
    key = int(combination.key())
    if ord("A") <= key <= ord("Z") or ord("0") <= key <= ord("9"):
        virtual_key = key
    elif int(Qt.Key.Key_F1) <= key <= int(Qt.Key.Key_F24) and key != int(Qt.Key.Key_F12):
        virtual_key = 0x70 + key - int(Qt.Key.Key_F1)
    else:
        raise ValueError("请搭配字母、数字或 F1–F24 功能键；F12 为系统保留键。")
    native_modifiers = 0
    for modifier, native_value in (
        (Qt.KeyboardModifier.AltModifier, 0x0001),
        (Qt.KeyboardModifier.ControlModifier, 0x0002),
        (Qt.KeyboardModifier.ShiftModifier, 0x0004),
    ):
        if modifiers & modifier:
            native_modifiers |= native_value
    return HotkeySpec(
        sequence.toString(QKeySequence.SequenceFormat.PortableText), native_modifiers, virtual_key
    )


def _load_user32():
    if sys.platform != "win32":
        return None
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    user32.RegisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.UINT, wintypes.UINT]
    user32.RegisterHotKey.restype = wintypes.BOOL
    user32.UnregisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.UnregisterHotKey.restype = wintypes.BOOL
    return user32


class _HotkeyEventFilter(QAbstractNativeEventFilter):
    def __init__(self, callback: Callable[[], None]) -> None:
        super().__init__()
        self.callback = callback
        self.active_id: int | None = None

    def nativeEventFilter(self, event_type, message) -> tuple[bool, int]:
        if (
            event_type not in (b"windows_generic_MSG", b"windows_dispatcher_MSG")
            or self.active_id is None
            or not message
        ):
            return False, 0
        native = wintypes.MSG.from_address(int(message))
        if native.message == WM_HOTKEY and native.wParam == self.active_id:
            self.callback()
            return True, 0
        return False, 0


class GlobalHotkey(QObject):
    activated = Signal()

    def __init__(self, parent: QObject | None = None, *, user32=None) -> None:
        super().__init__(parent)
        self._user32 = user32 if user32 is not None else _load_user32()
        self._filter = _HotkeyEventFilter(self.activated.emit)
        self._installed = False
        self.sequence = ""

    @property
    def supported(self) -> bool:
        return self._user32 is not None

    def register(self, sequence: str | QKeySequence) -> str:
        """Reserve a new chord before releasing the old one, preserving it on conflict."""
        spec = parse_hotkey(sequence)
        if not self.supported:
            raise OSError("全局快捷键目前仅支持 Windows；仍可使用窗口内快捷键。")
        if spec.text == self.sequence:
            return spec.text
        app = QCoreApplication.instance()
        if app is None:
            raise RuntimeError("请先启动 Qt 应用，再注册全局快捷键。")
        candidate_id = (
            _HOTKEY_IDS[1] if self._filter.active_id == _HOTKEY_IDS[0] else _HOTKEY_IDS[0]
        )
        if not self._user32.RegisterHotKey(
            None, candidate_id, spec.modifiers | MOD_NOREPEAT, spec.virtual_key
        ):
            raise OSError(f"{spec.text} 注册失败，可能已被其他应用占用，请更换组合。")
        previous_id = self._filter.active_id
        self._filter.active_id = candidate_id
        self.sequence = spec.text
        if not self._installed:
            app.installNativeEventFilter(self._filter)
            self._installed = True
        if previous_id is not None:
            self._user32.UnregisterHotKey(None, previous_id)
        return spec.text

    def close(self) -> None:
        """Release the OS registration and detach the native event filter once."""
        if self._filter.active_id is not None:
            self._user32.UnregisterHotKey(None, self._filter.active_id)
            self._filter.active_id = None
        if self._installed:
            app = QCoreApplication.instance()
            if app is not None:
                app.removeNativeEventFilter(self._filter)
            self._installed = False
        self.sequence = ""
