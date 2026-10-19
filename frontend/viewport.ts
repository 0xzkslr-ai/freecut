import type { Point } from "./selection";

export function imageViewport(
  width: number,
  height: number,
  imageWidth: number,
  imageHeight: number,
  crop: boolean,
) {
  if (!crop) return { x: 0, y: 0, width, height };
  const scale = Math.min(
    Math.max(1, width - 48) / imageWidth,
    Math.max(1, height - 120) / imageHeight,
  );
  const w = imageWidth * scale,
    h = imageHeight * scale;
  return { x: (width - w) / 2, y: (height - h) / 2, width: w, height: h };
}

// IPC uses the whole viewport; remap a fitted image selection to the same normalized coordinates.
export function viewportPoints(
  points: Point[],
  imageWidth: number,
  imageHeight: number,
  width: number,
  height: number,
): Point[] {
  return points.map(([x, y]) => [
    (x * width) / imageWidth,
    (y * height) / imageHeight,
  ]);
}
