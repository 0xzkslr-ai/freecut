import pytest
import shiboken6
from PySide6.QtCore import QPoint, QPointF, QRect, Qt
from PySide6.QtGui import QKeyEvent, QMouseEvent
from PySide6.QtTest import QTest

from freecut.overlay import SelectionOverlay
from freecut.selection import Mode


@pytest.fixture
def overlay(qapp, image):
    widget = SelectionOverlay(image, Mode.POLYGON, QRect(0, 0, 640, 480))
    widget.show()
    widget.activateWindow()
    widget.setFocus()
    qapp.processEvents()
    yield widget
    if shiboken6.isValid(widget):
        widget.close()
    qapp.processEvents()


def click(widget, x, y, button=Qt.MouseButton.LeftButton):
    QTest.mouseClick(widget, button, pos=QPoint(x, y))


def triangle(widget):
    for x, y in ((100, 100), (400, 100), (100, 350)):
        click(widget, x, y)


def test_polygon_close_accept_produces_transparent_crop(overlay):
    accepted = []
    cancelled = []
    overlay.accepted.connect(accepted.append)
    overlay.cancelled.connect(lambda: cancelled.append(True))
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    assert overlay.selection.complete
    assert overlay._toolbar.isVisible()
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    assert len(accepted) == 1
    assert not cancelled
    result = accepted[0]
    assert result.width() == 75
    assert result.pixelColor(result.width() - 2, result.height() - 2).alpha() == 0


def test_polygon_close_by_first_vertex(overlay):
    triangle(overlay)
    click(overlay, 102, 101)
    assert overlay.selection.complete


def test_polygon_double_click(overlay):
    triangle(overlay)
    QTest.mouseDClick(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 350))
    assert overlay.selection.complete
    QTest.mouseDClick(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 350))
    assert len(overlay.selection.points) == 3


def test_lasso_drag_release_and_reset(overlay):
    QTest.keyClick(overlay, Qt.Key.Key_L)
    QTest.mousePress(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 100))
    for point in (QPoint(350, 100), QPoint(400, 250), QPoint(100, 300)):
        QTest.mouseMove(overlay, point)
    QTest.mouseRelease(overlay, Qt.MouseButton.LeftButton, pos=QPoint(100, 100))
    assert overlay.selection.complete
    assert len(overlay.selection.points) >= 4
    click(overlay, 100, 100, Qt.MouseButton.RightButton)
    assert not overlay.selection.points
    assert overlay.selection.mode == Mode.LASSO


def test_invalid_lasso_can_be_redrawn(overlay):
    QTest.keyClick(overlay, Qt.Key.Key_L)
    click(overlay, 100, 100)
    assert not overlay.selection.complete
    assert "三个" in overlay._hint.text()
    QTest.keyClick(overlay, Qt.Key.Key_R)
    assert not overlay.selection.points


@pytest.mark.parametrize("key", [Qt.Key.Key_Backspace, Qt.Key.Key_Delete, Qt.Key.Key_Z])
def test_polygon_keyboard_undo(overlay, key):
    triangle(overlay)
    modifiers = (
        Qt.KeyboardModifier.ControlModifier
        if key == Qt.Key.Key_Z
        else Qt.KeyboardModifier.NoModifier
    )
    QTest.keyClick(overlay, key, modifiers)
    assert len(overlay.selection.points) == 2


def test_reopen_polygon_then_remove_vertex(overlay):
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    click(overlay, 500, 300, Qt.MouseButton.RightButton)
    assert not overlay.selection.complete
    assert len(overlay.selection.points) == 3
    click(overlay, 500, 300, Qt.MouseButton.RightButton)
    assert len(overlay.selection.points) == 2


def test_drag_polygon_vertex(overlay):
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    QTest.mousePress(overlay, Qt.MouseButton.LeftButton, pos=QPoint(400, 100))
    QTest.mouseMove(overlay, QPoint(450, 160))
    QTest.mouseRelease(overlay, Qt.MouseButton.LeftButton, pos=QPoint(450, 160))
    assert overlay.selection.points[1] == (450, 160)


def test_drag_whole_shape_preserves_geometry(overlay):
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    QTest.mousePress(overlay, Qt.MouseButton.LeftButton, pos=QPoint(160, 160))
    QTest.mouseMove(overlay, QPoint(200, 180))
    QTest.mouseRelease(overlay, Qt.MouseButton.LeftButton, pos=QPoint(200, 180))
    assert overlay.selection.points == [(140, 120), (440, 120), (140, 370)]


def test_click_outside_complete_selection_starts_over(overlay):
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Return)
    click(overlay, 550, 100)
    assert not overlay.selection.complete
    assert overlay.selection.points == [(550, 100)]


def test_mode_switch_and_hiding_hud(overlay):
    triangle(overlay)
    QTest.keyClick(overlay, Qt.Key.Key_Tab)
    assert not overlay._hint.isVisible()
    QTest.keyClick(overlay, Qt.Key.Key_Tab)
    assert overlay._hint.isVisible()
    QTest.keyClick(overlay, Qt.Key.Key_L)
    assert overlay.selection.mode == Mode.LASSO
    assert not overlay.selection.points
    QTest.keyClick(overlay, Qt.Key.Key_P)
    assert overlay.selection.mode == Mode.POLYGON
    assert not overlay.focusNextPrevChild(True)


def test_cancel_signal_only_once(overlay):
    calls = []
    overlay.cancelled.connect(lambda: calls.append(True))
    QTest.keyClick(overlay, Qt.Key.Key_Escape)
    assert calls == [True]


def test_invalid_vertex_edit_shows_error_without_accepting(overlay):
    calls = []
    overlay.accepted.connect(calls.append)
    overlay.selection.points = [(100, 100), (200, 100), (300, 100)]
    overlay.selection.complete = True
    overlay._accept()
    assert not calls
    assert "不共线" in overlay._hint.text()


def test_ignored_buttons_and_other_keys(overlay):
    event = QMouseEvent(
        QMouseEvent.Type.MouseButtonPress,
        QPointF(100, 100),
        QPointF(100, 100),
        Qt.MouseButton.MiddleButton,
        Qt.MouseButton.MiddleButton,
        Qt.KeyboardModifier.NoModifier,
    )
    overlay.mousePressEvent(event)
    overlay.mouseReleaseEvent(event)
    overlay.keyPressEvent(
        QKeyEvent(QKeyEvent.Type.KeyPress, Qt.Key.Key_F1, Qt.KeyboardModifier.NoModifier)
    )
    assert not overlay.selection.points


def test_polygon_cursor_preview_and_lasso_hint(overlay):
    click(overlay, 100, 100)
    QTest.mouseMove(overlay, QPoint(240, 200))
    assert not overlay.grab().isNull()
    QTest.keyClick(overlay, Qt.Key.Key_L)
    assert "自由套索" in overlay._hint.text()
    assert not overlay.grab().isNull()
