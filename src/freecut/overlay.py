from math import hypot

from PySide6.QtCore import QPointF, QRect, QRectF, QSizeF, Qt, Signal
from PySide6.QtGui import QColor, QImage, QKeyEvent, QMouseEvent, QPainter, QPen
from PySide6.QtWidgets import QFrame, QHBoxLayout, QLabel, QPushButton, QWidget

from freecut.imaging import crop_selection, selection_path
from freecut.selection import Mode, Selection, clamp_point, translate_points


class SelectionOverlay(QWidget):
    accepted = Signal(QImage)
    cancelled = Signal()

    def __init__(self, image: QImage, mode: Mode, geometry: QRect) -> None:
        super().__init__(
            None, Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint
        )
        self.setWindowTitle("FreeCut · 选择截图区域")
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.setGeometry(geometry)
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setCursor(Qt.CursorShape.CrossCursor)
        self.image = image.copy()
        self.image.setDevicePixelRatio(1)
        self.selection = Selection(mode)
        self._cursor = (0.0, 0.0)
        self._drawing = False
        self._drag_index: int | None = None
        self._move_anchor: tuple[float, float] | None = None
        self._original_points: list[tuple[float, float]] = []
        self._resolved = False
        self._hud_visible = True

        self._hint = QLabel(self)
        self._hint.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._hint.setStyleSheet(
            "background: #172b35; color: white; border-radius: 9px; padding: 12px 18px;"
        )
        self._toolbar = QFrame(self)
        self._toolbar.setStyleSheet(
            "QFrame { background: #172b35; border-radius: 9px; }"
            "QLabel { color: white; }"
            "QPushButton { color: white; background: #304650; border: 1px solid #71878c;"
            "border-radius: 6px; padding: 8px 12px; }"
            "QPushButton:hover { background: #47616b; }"
            "QPushButton:focus { border: 2px solid #69e2bb; }"
        )
        row = QHBoxLayout(self._toolbar)
        row.setContentsMargins(12, 10, 12, 10)
        row.setSpacing(8)
        self._size_label = QLabel()
        row.addWidget(self._size_label)
        for text, callback in (
            ("重选  R", self._reset),
            ("取消  Esc", self.close),
            ("确认  Enter", self._accept),
        ):
            button = QPushButton(text)
            button.setCursor(Qt.CursorShape.PointingHandCursor)
            button.setFocusPolicy(Qt.FocusPolicy.NoFocus)
            button.clicked.connect(callback)
            row.addWidget(button)
        self._refresh()

    def _refresh(self, message: str = "") -> None:
        if self.selection.complete:
            tip = "拖动选区移动位置"
            if self.selection.mode == Mode.POLYGON:
                tip += " · 拖动顶点调整轮廓"
            tip += "  |  Enter 确认 · R 重选 · Tab 隐藏提示 · Esc 取消"
            bounds = selection_path(self.selection.points).boundingRect()
            sx = self.image.width() / self.width()
            sy = self.image.height() / self.height()
            self._size_label.setText(
                f"约 {round(bounds.width() * sx)} × {round(bounds.height() * sy)} px"
            )
        elif self.selection.mode == Mode.LASSO:
            tip = "自由套索：按住左键绘制，松开闭合  |  P 多边形 · Tab 隐藏提示 · Esc 取消"
        else:
            tip = (
                "多边形：单击添加顶点，点击起点 / 双击 / Enter 闭合\n"
                "右键 / Backspace / Ctrl+Z 撤销  ·  L 套索  ·  Tab 隐藏提示  ·  Esc 取消"
            )
        self._hint.setText(message or tip)
        self._hint.adjustSize()
        self._hint.move(max(8, (self.width() - self._hint.width()) // 2), 20)
        self._hint.setVisible(self._hud_visible)
        self._toolbar.adjustSize()
        self._toolbar.move(
            max(0, (self.width() - self._toolbar.width()) // 2),
            max(0, self.height() - self._toolbar.height() - 24),
        )
        self._toolbar.setVisible(self.selection.complete and self._hud_visible)
        self.update()

    def _reset(self) -> None:
        self.selection.reset()
        self._drawing = False
        self._drag_index = None
        self._move_anchor = None
        self._refresh()

    def _undo(self) -> None:
        if self.selection.mode == Mode.LASSO:
            self._reset()
        else:
            self.selection.undo()
            self._refresh()

    def _finish(self) -> None:
        if self.selection.finish():
            self._refresh()
        else:
            self._refresh("选区需要至少三个不共线的点。请继续绘制，或按 R 重新选择。")

    def _accept(self) -> None:
        if not self.selection.complete:
            self._finish()
            return
        try:
            image = crop_selection(self.image, self.selection.points, QSizeF(self.size()))
        except ValueError as error:
            self._refresh(str(error))
            return
        self._resolved = True
        self.accepted.emit(image)
        self.close()

    def _point(self, event: QMouseEvent) -> tuple[float, float]:
        return clamp_point(
            (event.position().x(), event.position().y()), self.width(), self.height()
        )

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.RightButton:
            self._undo()
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        point = self._point(event)
        self._cursor = point
        if self.selection.complete:
            if self.selection.mode == Mode.POLYGON:
                for index, vertex in enumerate(self.selection.points):
                    if hypot(point[0] - vertex[0], point[1] - vertex[1]) <= 9:
                        self._drag_index = index
                        return
            if selection_path(self.selection.points).contains(QPointF(*point)):
                self._move_anchor = point
                self._original_points = self.selection.points.copy()
                self.setCursor(Qt.CursorShape.ClosedHandCursor)
                return
            self._reset()
        if self.selection.mode == Mode.LASSO:
            self.selection.start(point)
            self._drawing = True
        elif (
            len(self.selection.points) >= 3
            and hypot(
                point[0] - self.selection.points[0][0], point[1] - self.selection.points[0][1]
            )
            <= 9
        ):
            self._finish()
        else:
            self.selection.add(point)
        self._refresh()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        point = self._point(event)
        self._cursor = point
        if self._drag_index is not None:
            self.selection.points[self._drag_index] = point
        elif self._move_anchor is not None:
            self.selection.points = translate_points(
                self._original_points,
                (point[0] - self._move_anchor[0], point[1] - self._move_anchor[1]),
                self.width(),
                self.height(),
            )
        elif self._drawing:
            self.selection.add(point, minimum_distance=1.5)
        self._refresh()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if self._drawing:
            self.selection.add(self._point(event))
            self._drawing = False
            self._finish()
        elif self._drag_index is not None or self._move_anchor is not None:
            self.mouseMoveEvent(event)
            self._drag_index = None
            self._move_anchor = None
            self.setCursor(Qt.CursorShape.CrossCursor)
            self._refresh()

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton and self.selection.mode == Mode.POLYGON:
            if not self.selection.complete:
                self.selection.add(self._point(event))
                self._finish()

    def keyPressEvent(self, event: QKeyEvent) -> None:
        key = event.key()
        if key == Qt.Key.Key_Escape:
            self.close()
        elif key in (Qt.Key.Key_Enter, Qt.Key.Key_Return):
            self._accept()
        elif key in (Qt.Key.Key_Backspace, Qt.Key.Key_Delete) or (
            key == Qt.Key.Key_Z and event.modifiers() & Qt.KeyboardModifier.ControlModifier
        ):
            self._undo()
        elif key == Qt.Key.Key_R:
            self._reset()
        elif key in (Qt.Key.Key_L, Qt.Key.Key_P):
            self.selection.reset(Mode.LASSO if key == Qt.Key.Key_L else Mode.POLYGON)
            self._reset()
        elif key == Qt.Key.Key_Tab:
            self._hud_visible = not self._hud_visible
            self._refresh()
        else:
            super().keyPressEvent(event)

    def focusNextPrevChild(self, next: bool) -> bool:
        # Tab controls the HUD, rather than moving focus away from the canvas.
        return False

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.drawImage(QRectF(self.rect()), self.image, QRectF(self.image.rect()))
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        path = selection_path(self.selection.points, self.selection.complete)
        shade = selection_path(
            [(0, 0), (self.width(), 0), (self.width(), self.height()), (0, self.height())]
        )
        if self.selection.complete:
            shade = shade.subtracted(path)
        painter.fillPath(shade, QColor(8, 22, 29, 125))
        if self.selection.points:
            painter.setPen(QPen(QColor("#172b35"), 3))
            painter.drawPath(path)
            painter.setPen(QPen(QColor("#73f1ce"), 1.5))
            painter.drawPath(path)
            if self.selection.mode == Mode.POLYGON:
                if not self.selection.complete:
                    painter.setPen(QPen(QColor("#ffffff"), 1, Qt.PenStyle.DashLine))
                    painter.drawLine(QPointF(*self.selection.points[-1]), QPointF(*self._cursor))
                painter.setPen(QPen(QColor("#087f73"), 1.5))
                painter.setBrush(QColor("#ffffff"))
                for point in self.selection.points:
                    painter.drawEllipse(QPointF(*point), 4.5, 4.5)
        painter.end()

    def closeEvent(self, event) -> None:
        if not self._resolved:
            self._resolved = True
            self.cancelled.emit()
        event.accept()
