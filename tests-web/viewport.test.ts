import { describe, expect, it } from "vitest";
import { imageViewport, viewportPoints } from "../frontend/viewport";

describe("stitched image crop coordinates", () => {
  it("keeps normal captures aligned to the monitor viewport", () => {
    expect(imageViewport(1040, 720, 2080, 1440, false)).toEqual({
      x: 0,
      y: 0,
      width: 1040,
      height: 720,
    });
  });
  it("fits wide and tall images without changing their aspect ratio", () => {
    const wide = imageViewport(1040, 720, 10000, 1000, true);
    expect(wide.width).toBe(992);
    expect(wide.width / wide.height).toBeCloseTo(10);
    const tall = imageViewport(1040, 720, 1000, 10000, true);
    expect(tall.height).toBe(600);
    expect(tall.width / tall.height).toBeCloseTo(0.1);
    expect(tall.x).toBe((1040 - tall.width) / 2);
  });
  it("maps fitted crop points to the IPC viewport, preserving physical pixel coordinates", () => {
    expect(
      viewportPoints(
        [
          [0, 0],
          [248, 49.6],
          [992, 99.2],
        ],
        992,
        99.2,
        1040,
        720,
      ),
    ).toEqual([
      [0, 0],
      [260, 360],
      [1040, 720],
    ]);
    expect(viewportPoints([], 100, 100, 100, 100)).toEqual([]);
    expect(imageViewport(10, 10, 100, 100, true).width).toBe(1);
  });
});
