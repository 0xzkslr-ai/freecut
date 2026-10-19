from unittest.mock import Mock

import pytest
from PySide6.QtCore import QPointF, QSizeF
from PySide6.QtGui import QImage

from freecut import imaging
from freecut.imaging import crop_selection, image_mime_data, png_bytes, save_png, selection_path


def test_paths_open_closed_and_empty():
    assert selection_path([]).isEmpty()
    points = [(5, 5), (40, 5), (5, 40)]
    assert selection_path(points).contains(QPointF(10, 10))
    assert not selection_path(points).contains(QPointF(35, 35))
    assert selection_path(points, closed=False).elementCount() == 3
    assert selection_path(points).elementCount() == 4


def test_triangle_transparency_original_pixels_and_source_unchanged(image):
    original = image.copy()
    result = crop_selection(image, [(10, 10), (110, 10), (10, 90)], QSizeF(160, 120))
    assert result.size().width() == 100
    assert result.size().height() == 80
    assert result.pixelColor(5, 5) == image.pixelColor(15, 15)
    assert result.pixelColor(95, 75).alpha() == 0
    assert result.hasAlphaChannel()
    assert image == original
    assert any(
        0 < result.pixelColor(x, y).alpha() < 255
        for x in range(result.width())
        for y in range(result.height())
    )


def test_concave_polygon_retains_cutout(image):
    points = [(10, 10), (90, 10), (90, 40), (40, 40), (40, 90), (10, 90)]
    result = crop_selection(image, points, QSizeF(160, 120))
    assert result.pixelColor(10, 60).alpha() == 255
    assert result.pixelColor(60, 10).alpha() == 255
    assert result.pixelColor(60, 60).alpha() == 0


@pytest.mark.parametrize("ratio", [1, 1.25, 1.5, 2])
def test_dpi_conversion_preserves_physical_size(image, ratio):
    image.setDevicePixelRatio(ratio)
    result = crop_selection(
        image,
        [(0, 0), (80 / ratio, 0), (80 / ratio, 60 / ratio), (0, 60 / ratio)],
        QSizeF(160 / ratio, 120 / ratio),
    )
    assert result.width() == 80
    assert result.height() == 60
    assert result.devicePixelRatio() == 1
    assert result.pixelColor(20, 20).alpha() == 255
    assert image.devicePixelRatio() == ratio


def test_outside_coordinates_clip_to_image(image):
    result = crop_selection(image, [(-20, -20), (200, -20), (0, 200)], QSizeF(160, 120))
    assert result.width() <= image.width()
    assert result.height() <= image.height()
    assert result.pixelColor(0, 0).alpha() == 255


def test_self_intersection_uses_even_odd_fill(image):
    result = crop_selection(image, [(10, 10), (100, 100), (10, 100), (100, 10)], QSizeF(160, 120))
    assert result.pixelColor(45, 5).alpha() == 255
    assert result.pixelColor(5, 45).alpha() == 0


@pytest.mark.parametrize(
    ("points", "size"),
    [
        ([], QSizeF(160, 120)),
        ([(1, 1), (2, 2), (3, 3)], QSizeF(160, 120)),
        ([(0, 0), (1, 0), (0, 1)], QSizeF(160, 120)),
        ([(200, 200), (250, 200), (200, 250)], QSizeF(160, 120)),
        ([(0, 0), (50, 0), (0, 50)], QSizeF(0, 120)),
    ],
)
def test_invalid_selection_errors(image, points, size):
    with pytest.raises(ValueError):
        crop_selection(image, points, size)


def test_null_image_errors():
    with pytest.raises(ValueError):
        crop_selection(QImage(), [(0, 0), (20, 0), (0, 20)], QSizeF(100, 100))
    with pytest.raises(ValueError):
        png_bytes(QImage())


def test_png_round_trip_unicode_filename_and_atomic_replacement(image, tmp_path):
    result = crop_selection(image, [(10, 10), (110, 10), (10, 90)], QSizeF(160, 120))
    saved = save_png(result, tmp_path / "不规则截图")
    assert saved.name == "不规则截图.png"
    decoded = QImage(str(saved))
    assert decoded.size() == result.size()
    assert decoded.pixelColor(95, 75).alpha() == 0
    assert decoded.pixelColor(5, 5) == result.pixelColor(5, 5)
    save_png(image, saved)
    assert QImage(str(saved)).size() == image.size()
    assert save_png(image, tmp_path / "upper.PNG").name == "upper.PNG"


def test_unwritable_directory_and_failed_encoding(image, tmp_path):
    with pytest.raises(OSError, match="无法保存"):
        save_png(image, tmp_path / "missing" / "result.png")
    fake_image = Mock()
    fake_image.isNull.return_value = False
    fake_image.save.return_value = False
    with pytest.raises(OSError, match="编码失败"):
        png_bytes(fake_image)


@pytest.mark.parametrize("failure", ["write", "commit"])
def test_failed_atomic_write_keeps_existing_file(image, tmp_path, monkeypatch, failure):
    path = tmp_path / "existing.png"
    path.write_bytes(b"existing content")
    output = Mock()
    output.open.return_value = True
    output.write.return_value = -1 if failure == "write" else len(png_bytes(image))
    output.commit.return_value = False
    monkeypatch.setattr(imaging, "QSaveFile", lambda filename: output)
    with pytest.raises(OSError):
        save_png(image, path)
    assert path.read_bytes() == b"existing content"
    if failure == "write":
        output.cancelWriting.assert_called_once()


def test_clipboard_offers_png_with_alpha(image):
    result = crop_selection(image, [(10, 10), (110, 10), (10, 90)], QSizeF(160, 120))
    data = image_mime_data(result)
    assert data.hasImage()
    assert data.hasFormat("image/png")
    decoded = QImage.fromData(data.data("image/png"), "PNG")
    assert decoded.pixelColor(95, 75).alpha() == 0
