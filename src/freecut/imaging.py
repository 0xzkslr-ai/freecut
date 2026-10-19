"""Pixel-accurate, antialiased alpha masks and atomic PNG output."""

from pathlib import Path

from PySide6.QtCore import QIODevice, QMimeData, QPointF, QRectF, QSaveFile, QSizeF, Qt
from PySide6.QtGui import QImage, QPainter, QPainterPath

from freecut.selection import Point, has_area


def selection_path(points: list[Point], closed: bool = True) -> QPainterPath:
    path = QPainterPath()
    path.setFillRule(Qt.FillRule.OddEvenFill)
    if points:
        path.moveTo(QPointF(*points[0]))
        for point in points[1:]:
            path.lineTo(QPointF(*point))
        if closed:
            path.closeSubpath()
    return path


def crop_selection(image: QImage, points: list[Point], logical_size: QSizeF) -> QImage:
    """Map logical coordinates to source pixels, preserving original pixel density."""
    if image.isNull():
        raise ValueError("未获取到屏幕图像，请重新截图。")
    if logical_size.width() <= 0 or logical_size.height() <= 0:
        raise ValueError("截图尺寸无效。")
    if not has_area(points):
        raise ValueError("请圈出有面积的选区，至少需要三个不共线的点。")
    sx, sy = image.width() / logical_size.width(), image.height() / logical_size.height()
    path = selection_path([(x * sx, y * sy) for x, y in points])
    image_path = QPainterPath()
    image_path.addRect(QRectF(image.rect()))
    path = path.intersected(image_path)
    bounds = path.boundingRect().toAlignedRect().intersected(image.rect())
    if path.isEmpty() or bounds.width() < 2 or bounds.height() < 2:
        raise ValueError("选区太小或位于图像之外，请重新圈选。")

    mask = QImage(bounds.size(), QImage.Format.Format_ARGB32_Premultiplied)
    mask.fill(Qt.GlobalColor.transparent)
    painter = QPainter(mask)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.translate(-bounds.left(), -bounds.top())
    painter.fillPath(path, Qt.GlobalColor.white)
    painter.end()

    result = image.copy(bounds).convertToFormat(QImage.Format.Format_ARGB32_Premultiplied)
    result.setDevicePixelRatio(1)
    painter = QPainter(result)
    painter.setCompositionMode(QPainter.CompositionMode.CompositionMode_DestinationIn)
    painter.drawImage(0, 0, mask)
    painter.end()
    return result


def png_bytes(image: QImage) -> bytes:
    from PySide6.QtCore import QBuffer

    if image.isNull():
        raise ValueError("还没有可导出的截图。")
    buffer = QBuffer()
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    if not image.save(buffer, "PNG"):
        raise OSError("PNG 编码失败，请重新截图。")
    return bytes(buffer.data())


def save_png(image: QImage, filename: str | Path) -> Path:
    """Atomically replace a PNG only after successful encoding and writing."""
    path = Path(filename)
    if path.suffix.lower() != ".png":
        path = path.with_name(path.name + ".png")
    data = png_bytes(image)
    output = QSaveFile(str(path))
    if not output.open(QIODevice.OpenModeFlag.WriteOnly):
        raise OSError("无法保存图片，请检查目录是否存在、是否有写入权限。")
    if output.write(data) != len(data):
        output.cancelWriting()
        raise OSError("图片写入未完成，请检查磁盘空间。")
    if not output.commit():
        raise OSError("无法完成保存，请检查磁盘空间和文件权限。")
    return path


def image_mime_data(image: QImage) -> QMimeData:
    """Offer both native image data and explicit PNG bytes to clipboard consumers."""
    data = QMimeData()
    data.setImageData(image)
    data.setData("image/png", png_bytes(image))
    return data
