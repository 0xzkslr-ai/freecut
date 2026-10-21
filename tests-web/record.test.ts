import { describe, expect, it, vi } from "vitest";

vi.mock("../frontend/bridge", () => ({ backend: {} }));
import { recordingStatus, recordingTime } from "../frontend/record";

describe("recording duration", () => {
  it("formats whole seconds and hour boundaries", () => {
    expect(recordingTime(0)).toBe("00:00:00");
    expect(recordingTime(999)).toBe("00:00:00");
    expect(recordingTime(61_234)).toBe("00:01:01");
    expect(recordingTime(3_599_999)).toBe("00:59:59");
    expect(recordingTime(3_600_000)).toBe("01:00:00");
    expect(recordingTime(360_000_000)).toBe("100:00:00");
  });
  it("handles invalid elapsed times without showing NaN", () => {
    for (const value of [-1, NaN, Infinity, -Infinity]) {
      expect(recordingTime(value)).toBe("00:00:00");
    }
  });
});

describe("recording status", () => {
  it("keeps the boundary and controls in the same phase", () => {
    expect(
      recordingStatus({ starting: false, paused: false, finishing: false }),
    ).toEqual({ phase: "recording", label: "录制中" });
    expect(
      recordingStatus({ starting: false, paused: true, finishing: false }),
    ).toEqual({ phase: "paused", label: "已暂停" });
  });
  it("shows preparation and saving ahead of a stale pause state", () => {
    expect(
      recordingStatus({ starting: true, paused: true, finishing: false }),
    ).toEqual({ phase: "starting", label: "准备录制" });
    expect(
      recordingStatus({ starting: true, paused: true, finishing: true }),
    ).toEqual({ phase: "finishing", label: "正在保存" });
  });
});
