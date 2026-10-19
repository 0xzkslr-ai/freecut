import sys

from PySide6.QtCore import QEvent, QPointF, Qt
from PySide6.QtGui import QColor, QIcon, QPainter, QPainterPath, QPen, QPixmap
from PySide6.QtWidgets import QApplication

from freecut import __version__
from freecut.theme import PRIMARY, STYLESHEET
from freecut.window import MainWindow


def app_icon() -> QIcon:
    pixmap = QPixmap(64, 64)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(Qt.PenStyle.NoPen)
    painter.setBrush(QColor(PRIMARY))
    painter.drawRoundedRect(0, 0, 64, 64, 16, 16)
    path = QPainterPath(QPointF(17, 17))
    for x, y in ((45, 20), (48, 39), (32, 48), (17, 37)):
        path.lineTo(x, y)
    path.closeSubpath()
    painter.setBrush(Qt.BrushStyle.NoBrush)
    painter.setPen(QPen(QColor("white"), 2.5, Qt.PenStyle.DashLine))
    painter.drawPath(path)
    painter.setPen(Qt.PenStyle.NoPen)
    painter.setBrush(QColor("white"))
    painter.drawEllipse(QPointF(17, 17), 4, 4)
    painter.end()
    return QIcon(pixmap)


def main() -> int:
    app = QApplication(sys.argv)
    app.setApplicationName("FreeCut")
    app.setApplicationVersion(__version__)
    app.setOrganizationName("FreeCut")
    app.setStyle("Fusion")
    app.setStyleSheet(STYLESHEET)
    app.setWindowIcon(app_icon())
    window = MainWindow()
    window.show()
    exit_code = app.exec()
    # Destroy widgets while QApplication and its screen objects still exist.
    # Python signal closures can otherwise keep the main window alive until
    # interpreter shutdown, after the platform plugin has already been unloaded.
    window.close()
    window.deleteLater()
    app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
    return exit_code
