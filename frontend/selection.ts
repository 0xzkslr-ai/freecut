export type Point = [number, number];
export type Mode = "lasso" | "polygon" | "rectangle";

export function clampPoint(point: Point, width: number, height: number): Point {
  if (
    ![...point, width, height].every(Number.isFinite) ||
    width <= 0 ||
    height <= 0
  )
    throw new Error("坐标或屏幕尺寸无效。");
  return [
    Math.min(Math.max(point[0], 0), width),
    Math.min(Math.max(point[1], 0), height),
  ];
}

export function hasArea(points: Point[]): boolean {
  if (points.length < 3 || !points.every((p) => p.every(Number.isFinite)))
    return false;
  const [x0, y0] = points[0];
  const direction = points.find(([x, y]) => x !== x0 || y !== y0);
  if (!direction) return false;
  const [dx, dy] = [direction[0] - x0, direction[1] - y0];
  return points.some(
    ([x, y]) => Math.abs(dx * (y - y0) - dy * (x - x0)) > 0.01,
  );
}

export function bounds(points: Point[]) {
  if (!points.length) return { x: 0, y: 0, width: 0, height: 0 };
  let x = Infinity,
    y = Infinity,
    right = -Infinity,
    bottom = -Infinity;
  for (const [px, py] of points) {
    x = Math.min(x, px);
    y = Math.min(y, py);
    right = Math.max(right, px);
    bottom = Math.max(bottom, py);
  }
  return { x, y, width: right - x, height: bottom - y };
}

export function translatePoints(
  points: Point[],
  [dx, dy]: Point,
  width: number,
  height: number,
): Point[] {
  if (!points.length) return [];
  const b = bounds(points);
  dx = Math.min(Math.max(dx, -b.x), width - b.x - b.width);
  dy = Math.min(Math.max(dy, -b.y), height - b.y - b.height);
  return points.map(([x, y]) => [x + dx, y + dy]);
}

export function containsPoint(points: Point[], [x, y]: Point): boolean {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const [xi, yi] = points[i],
      [xj, yj] = points[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi)
      inside = !inside;
  }
  return inside;
}

const distance = (a: Point, b: Point) => Math.hypot(a[0] - b[0], a[1] - b[1]);

function rectanglePoints(a: Point, b: Point): Point[] {
  const left = Math.min(a[0], b[0]),
    top = Math.min(a[1], b[1]);
  const right = Math.max(a[0], b[0]),
    bottom = Math.max(a[1], b[1]);
  return [
    [left, top],
    [right, top],
    [right, bottom],
    [left, bottom],
  ];
}

export class Selection {
  points: Point[] = [];
  complete = false;
  drawing = false;
  private vertex: number | null = null;
  private anchor: Point | null = null;
  private original: Point[] = [];
  private rectangleOrigin: Point | null = null;

  constructor(public mode: Mode = "lasso") {}

  reset(mode = this.mode) {
    this.mode = mode;
    this.points = [];
    this.complete = this.drawing = false;
    this.vertex = this.anchor = null;
    this.original = [];
    this.rectangleOrigin = null;
  }

  add(point: Point, minimumDistance = 0) {
    if (
      this.complete ||
      (this.points.length &&
        distance(point, this.points.at(-1)!) <= minimumDistance)
    )
      return;
    this.points.push(point);
  }

  finish(): boolean {
    this.complete = hasArea(this.points);
    return this.complete;
  }

  undo() {
    this.drawing = false;
    this.anchor = this.vertex = null;
    if (this.mode !== "polygon") this.reset();
    else if (this.complete) this.complete = false;
    else this.points.pop();
  }

  down(point: Point) {
    if (this.complete) {
      if (this.mode !== "lasso") {
        const vertex = this.points.findIndex((p) => distance(p, point) <= 9);
        if (vertex !== -1) {
          this.vertex = vertex;
          this.original = this.points.map((p) => [...p]);
          return;
        }
      }
      if (containsPoint(this.points, point)) {
        this.anchor = point;
        this.original = this.points.map((p) => [...p]);
        return;
      }
      this.reset();
    }
    if (this.mode !== "polygon") {
      this.rectangleOrigin = this.mode === "rectangle" ? point : null;
      this.points = this.rectangleOrigin
        ? rectanglePoints(point, point)
        : [point];
      this.drawing = true;
    } else if (this.points.length >= 3 && distance(point, this.points[0]) <= 9)
      this.finish();
    else this.add(point);
  }

  move(point: Point, width: number, height: number) {
    if (this.vertex !== null) {
      if (this.mode === "rectangle")
        this.points = rectanglePoints(
          point,
          this.original[(this.vertex + 2) % 4],
        );
      else this.points[this.vertex] = point;
    } else if (this.anchor)
      this.points = translatePoints(
        this.original,
        [point[0] - this.anchor[0], point[1] - this.anchor[1]],
        width,
        height,
      );
    else if (this.drawing) {
      if (this.rectangleOrigin)
        this.points = rectanglePoints(this.rectangleOrigin, point);
      else this.add(point, 1.5);
    }
  }

  up(point: Point, width: number, height: number) {
    if (this.drawing) {
      if (this.rectangleOrigin) this.move(point, width, height);
      else this.add(point);
      this.drawing = false;
      this.finish();
    } else {
      this.move(point, width, height);
      if (this.mode === "rectangle" && this.vertex !== null) this.finish();
    }
    this.vertex = this.anchor = null;
    this.original = [];
    this.rectangleOrigin = null;
  }
}
