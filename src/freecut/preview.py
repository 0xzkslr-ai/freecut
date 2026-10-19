from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QBrush, QColor, QImage, QPainter, QPen, QPixmap
from PySide6.QtWidgets import QWidget


class Preview(QWidget):
    """Fit-to-window preview; checkerboard is never included in the exported image."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.image = QImage()
        self.setMinimumSize(300, 280)
        self.setAccessibleName("透明截图预览")
        tile = QPixmap(24, 24)
        tile.fill(QColor("#f1f4f5"))
        painter = QPainter(tile)
        painter.fillRect(0, 0, 12, 12, QColor("#e3e9ec"))
        painter.fillRect(12, 12, 12, 12, QColor("#e3e9ec"))
        painter.end()
        self._checker = QBrush(tile)

    def set_image(self, image: QImage) -> None:
        self.image = image.copy()
        self.update()

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.fillRect(self.rect(), self._checker)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        if self.image.isNull():
            box = QRectF(self.rect()).adjusted(32, 32, -32, -32)
            painter.setPen(QPen(QColor("#94aaa9"), 1.5, Qt.PenStyle.DashLine))
            painter.setBrush(QColor(255, 255, 255, 210))
            painter.drawRoundedRect(box, 16, 16)
            painter.setPen(QColor("#526572"))
            painter.drawText(
                box.adjusted(20, 20, -20, -20),
                Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
                "只截取你想要的形状\n\n选择左侧工具，开始一次截图。\n选区以外的部分会变为透明。",
            )
        else:
            available = self.rect().adjusted(24, 24, -24, -24)
            scale = min(
                available.width() / self.image.width(),
                available.height() / self.image.height(),
                1.0,
            )
            width, height = self.image.width() * scale, self.image.height() * scale
            target = QRectF((self.width() - width) / 2, (self.height() - height) / 2, width, height)
            painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
            painter.drawImage(target, self.image, QRectF(self.image.rect()))
        painter.end()
