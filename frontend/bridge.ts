import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Mode, Point } from "./selection";

export interface Monitor {
  id: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
}
export interface Session {
  id: number;
  width: number;
  height: number;
  mode: Mode;
  crop?: boolean;
  recording?: boolean;
}
export interface Preview {
  id: number;
  width: number;
  height: number;
}
export interface Settings {
  mode: Mode;
  hotkey: string;
}
export interface ScrollInfo {
  id: number;
  width: number;
  height: number;
  frames: number;
  paused: boolean;
  finishing: boolean;
  recovering: boolean;
  message: string;
  hotkey: string;
  coverage: {
    bounds: CoverageRect;
    viewport: CoverageRect;
    regions: CoverageRect[];
    unplaced: number;
  };
}
export interface CoverageRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface InitialState {
  settings: Settings;
  monitors: Monitor[];
  preview: Preview | null;
  hotkeyError: string | null;
  trayAvailable: boolean;
  recordedVideo?: RecordingResult | null;
}

export interface RecordingInfo {
  id: number;
  width: number;
  height: number;
  elapsedMs: number;
  frames: number;
  paused: boolean;
  starting: boolean;
  finishing: boolean;
  message: string;
  hotkey: string;
  region?: CoverageRect;
  monitorName?: string;
}
export interface RecordingResult {
  id: number;
  width: number;
  height: number;
  elapsedMs: number;
  bytes: number;
  path: string;
  warning: string | null;
}

export const imageUrl = (id: number, kind: "source" | "preview") =>
  convertFileSrc(`${kind}-${id}`, "freecut");
export const backend = {
  init: () => invoke<InitialState>("initial_state"),
  ready: () => invoke<void>("frontend_ready"),
  frontendFailed: (message: string) =>
    invoke<void>("frontend_failed", { message }),
  monitors: () => invoke<Monitor[]>("list_monitors"),
  capture: (monitorId: string | null) =>
    invoke<void>("start_capture", { monitorId }),
  recordSelection: (monitorId: string | null) =>
    invoke<void>("start_capture", { monitorId, recording: true }),
  session: () => invoke<Session | null>("current_session"),
  showOverlay: (id: number) => invoke<void>("show_overlay", { id }),
  cancel: (id: number) => invoke<void>("cancel_capture", { id }),
  accept: (id: number, points: Point[], width: number, height: number) =>
    invoke<Preview>("accept_capture", { id, points, width, height }),
  scroll: (id: number, points: Point[], width: number, height: number) =>
    invoke<void>("start_scroll", { id, points, width, height }),
  scrollState: () => invoke<ScrollInfo | null>("scroll_state"),
  pauseScroll: (id: number, paused: boolean) =>
    invoke<void>("pause_scroll", { id, paused }),
  finishScroll: (id: number) => invoke<void>("finish_scroll", { id }),
  cancelScroll: (id: number) => invoke<void>("cancel_scroll", { id }),
  record: (id: number, points: Point[], width: number, height: number) =>
    invoke<boolean>("start_recording", { id, points, width, height }),
  recordingState: () => invoke<RecordingInfo | null>("recording_state"),
  recordingRegionReady: () => invoke<void>("recording_region_ready"),
  recordingRegionShown: (id: number) =>
    invoke<void>("recording_region_shown", { id }),
  highlightRecordingRegion: (id: number) =>
    invoke<void>("highlight_recording_region", { id }),
  pauseRecording: (id: number, paused: boolean) =>
    invoke<void>("pause_recording", { id, paused }),
  finishRecording: (id: number) => invoke<void>("finish_recording", { id }),
  cancelRecording: (id: number) => invoke<void>("cancel_recording", { id }),
  openRecording: (id: number, folder: boolean) =>
    invoke<void>("open_recording", { id, folder }),
  edit: () => invoke<void>("edit_result"),
  mode: (mode: Mode) => invoke<void>("set_mode", { mode }),
  hotkey: (hotkey: string) => invoke<string>("set_hotkey", { hotkey }),
  copy: () => invoke<void>("copy_image"),
  save: () => invoke<string | null>("save_image"),
  background: () => invoke<void>("hide_to_tray"),
  quit: () => invoke<void>("quit_app"),
  on: <T>(event: string, handler: (data: T) => void) =>
    listen<T>(event, ({ payload }) => handler(payload)),
};
