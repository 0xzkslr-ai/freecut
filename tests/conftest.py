import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest  # noqa: E402
from PySide6.QtCore import QEvent  # noqa: E402
from PySide6.QtGui import QColor, QImage  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

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
