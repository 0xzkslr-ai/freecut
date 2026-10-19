import { describe, expect, it } from "vitest";
import {
  Selection,
  bounds,
  clampPoint,
  containsPoint,
  hasArea,
  translatePoints,
  type Point,
} from "../frontend/selection";
import { hotkeyFromEvent } from "../frontend/hotkey";

const square: Point[] = [
  [10, 10],
  [80, 10],
  [80, 80],
  [10, 80],
];
describe("geometry", () => {
  it("clamps finite screen coordinates including the edges", () => {
    expect(clampPoint([-2, 110], 100, 100)).toEqual([0, 100]);
    expect(clampPoint([30, 40], 100, 100)).toEqual([30, 40]);
    for (const args of [
      [[NaN, 0], 100, 100],
      [[0, Infinity], 100, 100],
      [[0, 0], 0, 100],
      [[0, 0], 100, -1],
    ] as [Point, number, number][])
      expect(() => clampPoint(...args)).toThrow();
  });
  it("rejects degenerate selections and accepts self intersections", () => {
    for (const points of [
      [],
      [[0, 0]],
      [
        [0, 0],
        [0, 0],
        [0, 0],
      ],
      [
        [0, 0],
        [1, 1],
        [2, 2],
      ],
      [
        [0, 0],
        [1, NaN],
        [1, 2],
      ],
    ] as Point[][])
      expect(hasArea(points)).toBe(false);
    expect(
      hasArea([
        [0, 0],
        [50, 50],
        [50, 0],
        [0, 50],
      ]),
    ).toBe(true);
    expect(hasArea(square)).toBe(true);
  });
  it("bounds empty and negative coordinates", () => {
    expect(bounds([])).toEqual({ x: 0, y: 0, width: 0, height: 0 });
    expect(
      bounds([
        [-1, -2],
        [4, 8],
      ]),
    ).toEqual({ x: -1, y: -2, width: 5, height: 10 });
  });
  it("translates without deforming at each edge", () => {
    expect(translatePoints([], [1, 1], 100, 100)).toEqual([]);
    expect(translatePoints(square, [999, -999], 100, 100)).toEqual([
      [30, 0],
      [100, 0],
      [100, 70],
      [30, 70],
    ]);
    expect(translatePoints(square, [-999, 999], 100, 100)).toEqual([
      [0, 30],
      [70, 30],
      [70, 100],
      [0, 100],
    ]);
    expect(square[0]).toEqual([10, 10]);
  });
  it("uses even odd containment for concave and self intersecting paths", () => {
    expect(containsPoint([], [1, 1])).toBe(false);
    expect(containsPoint(square, [30, 40])).toBe(true);
    expect(containsPoint(square, [90, 30])).toBe(false);
    const concave: Point[] = [
      [0, 0],
      [100, 0],
      [100, 30],
      [30, 30],
      [30, 100],
      [0, 100],
    ];
    expect(containsPoint(concave, [10, 80])).toBe(true);
    expect(containsPoint(concave, [80, 80])).toBe(false);
  });
});
describe("selection interaction", () => {
  it("draws, filters close points, closes and resets a lasso", () => {
    const s = new Selection();
    s.down([10, 10]);
    s.move([10.5, 10], 100, 100);
    expect(s.points).toHaveLength(1);
    s.move([80, 10], 100, 100);
    s.move([80, 80], 100, 100);
    s.up([10, 80], 100, 100);
    expect(s.complete).toBe(true);
    expect(s.drawing).toBe(false);
    s.add([3, 4]);
    expect(s.points).toHaveLength(4);
    s.undo();
    expect(s.points).toEqual([]);
    expect(s.complete).toBe(false);
  });
  it("rejects a click without area", () => {
    const s = new Selection();
    s.down([1, 1]);
    s.up([1, 1], 100, 100);
    expect(s.complete).toBe(false);
  });
  it("closes a polygon by clicking its origin, then reopens before removing vertices", () => {
    const s = new Selection("polygon");
    for (const p of square) {
      s.down(p);
      s.up(p, 100, 100);
    }
    expect(s.complete).toBe(false);
    s.down([12, 12]);
    expect(s.complete).toBe(true);
    s.undo();
    expect(s.complete).toBe(false);
    expect(s.points).toHaveLength(4);
    s.undo();
    expect(s.points).toHaveLength(3);
    s.reset("lasso");
    expect(s.mode).toBe("lasso");
    expect(s.points).toEqual([]);
  });
  it("moves complete regions and clamps whole shapes", () => {
    const s = new Selection();
    s.points = square.map((p) => [...p]);
    s.finish();
    s.down([40, 40]);
    s.move([100, 100], 100, 100);
    s.up([100, 100], 100, 100);
    expect(s.points).toEqual([
      [30, 30],
      [100, 30],
      [100, 100],
      [30, 100],
    ]);
    expect(s.complete).toBe(true);
  });
  it("edits a polygon vertex without moving the others", () => {
    const s = new Selection("polygon");
    s.points = square.map((p) => [...p]);
    s.finish();
    s.down([11, 11]);
    s.move([2, 4], 100, 100);
    s.up([3, 5], 100, 100);
    expect(s.points[0]).toEqual([3, 5]);
    expect(s.points[1]).toEqual([80, 10]);
    expect(s.complete).toBe(true);
  });
  it("starts over outside a closed selection and undoes empty polygons safely", () => {
    const s = new Selection("polygon");
    s.undo();
    s.points = square.map((p) => [...p]);
    s.finish();
    s.down([95, 95]);
    expect(s.points).toEqual([[95, 95]]);
    expect(s.complete).toBe(false);
    s.add([95, 95]);
    expect(s.points).toHaveLength(1);
    s.move([98, 98], 100, 100);
    expect(s.points).toHaveLength(1);
  });
});
describe("rectangle interaction", () => {
  it.each([
    [
      [10, 20],
      [80, 90],
    ],
    [
      [80, 90],
      [10, 20],
    ],
    [
      [80, 20],
      [10, 90],
    ],
    [
      [10, 90],
      [80, 20],
    ],
  ] as [Point, Point][])(
    "normalizes either drag direction from %j to %j",
    (start, end) => {
      const s = new Selection("rectangle");
      s.down(start);
      s.move([40, 50], 100, 100);
      expect(s.points).toHaveLength(4);
      s.up(end, 100, 100);
      expect(s.complete).toBe(true);
      expect(s.drawing).toBe(false);
      expect(s.points).toEqual([
        [10, 20],
        [80, 20],
        [80, 90],
        [10, 90],
      ]);
    },
  );
  it.each([
    [10, 10],
    [10, 80],
    [80, 10],
  ] as Point[])("rejects a zero-area rectangle ending at %j", (x, y) => {
    const s = new Selection("rectangle");
    s.down([10, 10]);
    s.up([x, y], 100, 100);
    expect(s.complete).toBe(false);
  });
  it("moves the rectangle as a whole without deforming at the screen edge", () => {
    const s = new Selection("rectangle");
    s.down([10, 10]);
    s.up([80, 80], 100, 100);
    s.down([40, 40]);
    s.up([100, 100], 100, 100);
    expect(s.points).toEqual([
      [30, 30],
      [100, 30],
      [100, 100],
      [30, 100],
    ]);
    expect(s.complete).toBe(true);
  });
  it.each([0, 1, 2, 3])(
    "resizes corner %i around its fixed opposite, including crossing it",
    (corner) => {
      const s = new Selection("rectangle");
      s.down([10, 10]);
      s.up([80, 80], 100, 100);
      const opposite = s.points[(corner + 2) % 4];
      s.down(s.points[corner]);
      s.move([90, 90], 100, 100);
      s.up([0, 100], 100, 100);
      expect(s.points).toEqual([
        [0, opposite[1]],
        [opposite[0], opposite[1]],
        [opposite[0], 100],
        [0, 100],
      ]);
      expect(s.complete).toBe(true);
    },
  );
  it("rejects a rectangle resized to a line, and starts a new one outside the old region", () => {
    const s = new Selection("rectangle");
    s.down([10, 10]);
    s.up([80, 80], 100, 100);
    s.down([10, 10]);
    s.up([80, 10], 100, 100);
    expect(s.complete).toBe(false);
    s.down([20, 20]);
    s.up([60, 60], 100, 100);
    s.down([90, 90]);
    s.up([100, 100], 100, 100);
    expect(s.points).toEqual([
      [90, 90],
      [100, 90],
      [100, 100],
      [90, 100],
    ]);
  });
  it("undo and tool changes reset rectangles instead of reopening them as polygons", () => {
    const s = new Selection("rectangle");
    s.down([10, 10]);
    s.up([80, 80], 100, 100);
    s.undo();
    expect(s.points).toEqual([]);
    expect(s.complete).toBe(false);
    s.down([10, 10]);
    s.undo();
    s.move([80, 80], 100, 100);
    expect(s.points).toEqual([]);
    s.reset("polygon");
    s.down([30, 40]);
    expect(s.points).toEqual([[30, 40]]);
    expect(s.drawing).toBe(false);
  });
});
describe("shortcut recording", () => {
  const event = {
    code: "KeyA",
    ctrlKey: true,
    altKey: true,
    shiftKey: false,
    metaKey: false,
  };
  it("records physical keys, not keyboard-layout dependent characters", () => {
    expect(hotkeyFromEvent(event)).toBe("Ctrl+Alt+A");
    expect(
      hotkeyFromEvent({
        ...event,
        altKey: false,
        shiftKey: true,
        code: "Digit1",
      }),
    ).toBe("Ctrl+Shift+1");
    expect(hotkeyFromEvent({ ...event, ctrlKey: false, code: "F24" })).toBe(
      "Alt+F24",
    );
  });
  it("rejects reserved or ambiguous keys", () => {
    for (const code of ["F12", "F25", "F0", "Numpad1", "Space", "ControlLeft"])
      expect(hotkeyFromEvent({ ...event, code })).toBeNull();
    expect(hotkeyFromEvent({ ...event, metaKey: true })).toBeNull();
    expect(
      hotkeyFromEvent({
        ...event,
        ctrlKey: false,
        altKey: false,
        shiftKey: true,
      }),
    ).toBeNull();
  });
});
