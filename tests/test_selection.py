import pytest

from freecut.selection import Mode, Selection, clamp_point, has_area, translate_points


@pytest.mark.parametrize(
    ("point", "expected"),
    [((5, 8), (5, 8)), ((-5, 200), (0, 80)), ((100, -1), (100, 0))],
)
def test_clamp_point(point, expected):
    assert clamp_point(point, 100, 80) == expected


@pytest.mark.parametrize(
    ("point", "width", "height"),
    [((float("nan"), 1), 2, 2), ((1, float("inf")), 2, 2), ((1, 1), 0, 2), ((1, 1), 2, -1)],
)
def test_invalid_coordinates(point, width, height):
    with pytest.raises(ValueError):
        clamp_point(point, width, height)


@pytest.mark.parametrize(
    ("points", "expected"),
    [
        ([], False),
        ([(0, 0), (1, 1)], False),
        ([(0, 0), (1, 1), (2, 2)], False),
        ([(0, 0)] * 4, False),
        ([(0, 0), (0, 0), (2, 0), (0, 2)], True),
        ([(0, 0), (2, 0), (0, 2)], True),
        ([(0, 0), (2, 2), (0, 2), (2, 0)], True),
    ],
)
def test_selection_area(points, expected):
    assert has_area(points) is expected


def test_selection_lifecycle():
    selection = Selection()
    selection.start((0, 0))
    selection.add((0, 0))
    selection.add((0.5, 0), minimum_distance=1)
    assert selection.points == [(0, 0)]
    assert not selection.finish()
    selection.add((20, 0))
    selection.add((0, 20))
    assert selection.finish()
    selection.add((40, 40))
    assert len(selection.points) == 3
    selection.undo()
    assert not selection.complete
    assert len(selection.points) == 3
    selection.undo()
    assert len(selection.points) == 2
    selection.reset(Mode.POLYGON)
    selection.undo()
    assert selection.mode == Mode.POLYGON
    assert not selection.points
    selection.start((4, 5))
    selection.reset()
    assert selection.mode == Mode.POLYGON


@pytest.mark.parametrize(
    ("delta", "expected"),
    [
        ((5, 3), [(15, 13), (35, 13), (15, 33)]),
        ((-100, -100), [(0, 0), (20, 0), (0, 20)]),
        ((100, 100), [(80, 60), (100, 60), (80, 80)]),
    ],
)
def test_translation_preserves_shape_at_edges(delta, expected):
    points = [(10, 10), (30, 10), (10, 30)]
    assert translate_points(points, delta, 100, 80) == expected
    assert points[0] == (10, 10)
    assert translate_points([], delta, 100, 80) == []
