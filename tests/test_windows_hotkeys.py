"""Run directly on Windows to verify real registration and Qt native message delivery."""

import ctypes
import os
import sys
import unittest
from ctypes import wintypes

from PySide6.QtCore import QElapsedTimer
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication

from freecut.hotkeys import WM_HOTKEY, GlobalHotkey

ENABLED = sys.platform == "win32" and (
    __name__ == "__main__" or os.environ.get("FREECUT_WINDOWS_TEST") == "1"
)


@unittest.skipUnless(ENABLED, "Run directly on Windows, or set FREECUT_WINDOWS_TEST=1")
class WindowsHotkeyIntegrationTest(unittest.TestCase):
    def test_changed_shortcuts_reach_qt_and_release_the_os_registration(self):
        app = QApplication.instance() or QApplication([])
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.GetCurrentThreadId.restype = wintypes.DWORD
        user32.PostThreadMessageW.argtypes = [
            wintypes.DWORD,
            wintypes.UINT,
            wintypes.WPARAM,
            wintypes.LPARAM,
        ]
        user32.PostThreadMessageW.restype = wintypes.BOOL
        service = GlobalHotkey()
        received = []
        service.activated.connect(lambda: received.append(service.sequence))
        previous_id = None
        sequences = ["Ctrl+Alt+Shift+F22", "Ctrl+Alt+Shift+F23", "Ctrl+Alt+Shift+F24"]
        try:
            for index, sequence in enumerate(sequences):
                try:
                    service.register(sequence)
                except OSError as error:
                    self.skipTest(f"Native test shortcut unavailable: {error}")
                # A queued notification for the old registration must be ignored.
                if previous_id is not None:
                    self.assertTrue(
                        user32.PostThreadMessageW(
                            kernel32.GetCurrentThreadId(), WM_HOTKEY, previous_id, 0
                        )
                    )
                previous_id = service._filter.active_id
                self.assertTrue(
                    user32.PostThreadMessageW(
                        kernel32.GetCurrentThreadId(), WM_HOTKEY, previous_id, 0
                    )
                )
                elapsed = QElapsedTimer()
                elapsed.start()
                while len(received) <= index and elapsed.elapsed() < 1000:
                    QTest.qWait(1)
                self.assertEqual(received, sequences[: index + 1])
        finally:
            service.close()
        replacement = GlobalHotkey()
        try:
            self.assertEqual(replacement.register(sequences[-1]), sequences[-1])
        finally:
            replacement.close()
        app.processEvents()


if __name__ == "__main__":
    unittest.main()
