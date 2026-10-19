"""Selection state, independent of the window system."""

from dataclasses import dataclass, field
from enum import Enum
from math import hypot, isfinite

Point = tuple[float, float]


class Mode(Enum):
    LASSO = "自由套索"
    POLYGON = "多边形"


def clamp_point(point: Point, width: float, height: float) -> Point:
    """Keep a finite point within logical screen coordinates, including its edges."""
    if not all(isfinite(value) for value in (*point, width, height)):
        raise ValueError("坐标必须是有限数值。")
    if width <= 0 or height <= 0:
        raise ValueError("屏幕尺寸必须大于零。")
    return min(max(point[0], 0), width), min(max(point[1], 0), height)


def has_area(points: list[Point]) -> bool:
    """Reject clicks and collinear strokes; permit self-intersecting lassos."""
    if len(points) < 3:
        return False
    origin = points[0]
    direction = next((point for point in points[1:] if point != origin), None)
    if direction is None:
        return False
    dx, dy = direction[0] - origin[0], direction[1] - origin[1]
    return any(abs(dx * (y - origin[1]) - dy * (x - origin[0])) > 0.01 for x, y in points)


def translate_points(points: list[Point], delta: Point, width: float, height: float) -> list[Point]:
    """Move a selection as a whole without deforming it against screen edges."""
    if not points:
        return []
    dx = min(max(delta[0], -min(x for x, _ in points)), width - max(x for x, _ in points))
    dy = min(max(delta[1], -min(y for _, y in points)), height - max(y for _, y in points))
    return [(x + dx, y + dy) for x, y in points]


@dataclass
class Selection:
    mode: Mode = Mode.LASSO
    points: list[Point] = field(default_factory=list)
    complete: bool = False

    def start(self, point: Point) -> None:
        self.points = [point]
        self.complete = False

    def add(self, point: Point, minimum_distance: float = 0) -> None:
        if self.complete:
            return
        if (
            self.points
            and hypot(point[0] - self.points[-1][0], point[1] - self.points[-1][1])
            <= minimum_distance
        ):
            return
        self.points.append(point)

    def finish(self) -> bool:
        self.complete = has_area(self.points)
        return self.complete

    def undo(self) -> None:
        if self.complete:
            self.complete = False
        elif self.points:
            self.points.pop()

    def reset(self, mode: Mode | None = None) -> None:
        if mode is not None:
            self.mode = mode
        self.points.clear()
        self.complete = False
