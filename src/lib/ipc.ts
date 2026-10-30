import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  CreateResult,
  CursorLocation,
  OverlaySnapshot,
  Rect,
  Settings,
  SettingsPayload,
  UpdateInfo,
  WindowHit,
} from "../types";

/**
 * Thin typed wrappers around Rust commands.
 * Tauri converts camelCase arguments to snake_case automatically.
 */
export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  startCapture: () => invoke<void>("start_capture"),
  cancelCapture: () => invoke<void>("cancel_capture"),

  overlaySnapshot: (monitorId: number) =>
    invoke<OverlaySnapshot>("overlay_snapshot", { monitorId }),
  overlayReady: (monitorId: number) => invoke<void>("overlay_ready", { monitorId }),

  /** Monitor screenshot as raw RGBA with an 8-byte width/height header. */
  overlayImage: async (monitorId: number) => {
    return new Uint8Array(await invoke<ArrayBuffer>("overlay_image", { monitorId }));
  },

  windowsUnderCursor: (monitorId: number, x: number, y: number) =>
    invoke<WindowHit[]>("windows_under_cursor", { monitorId, x, y }),

  cursorLocation: () => invoke<CursorLocation | null>("cursor_location"),

  overlayCommit: (monitorId: number, rect: Rect, mode: "edit" | "copy" = "edit") =>
    invoke<CreateResult>("overlay_commit", {
      monitorId,
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
      mode,
    }),

  copyImage: (bytes: Uint8Array) => invoke<void>("copy_image", { bytes: Array.from(bytes) }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  saveImage: (path: string, bytes: Uint8Array) =>
    invoke<void>("save_image", { path, bytes: Array.from(bytes) }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
  openFolder: () => invoke<string>("open_folder"),
  quit: () => invoke<void>("quit_app"),

  /** Raw RGBA pixels for the pending editor image. */
  imageInfo: () => invoke<[number, number] | null>("image_info"),
  imageRgba: async () => new Uint8Array(await invoke<number[] | Uint8Array>("image_rgba")),

  getSettings: () => invoke<SettingsPayload>("get_settings"),
  saveSettings: (settings: Settings) => invoke<SettingsPayload>("save_settings", { next: settings }),
  resetShortcut: () => invoke<SettingsPayload>("reset_shortcut"),
  pickSaveDir: () => invoke<string | null>("pick_save_dir"),
  openSettings: () => invoke<void>("open_settings"),
  checkForUpdates: () => invoke<UpdateInfo>("check_for_updates"),
  openExternalUrl: (url: string) => invoke<void>("open_external_url", { url }),
  checkScreenPermission: () => invoke<boolean>("check_screen_permission"),
  openScreenPermissionSettings: () => invoke<void>("open_screen_permission_settings"),
  hideMain: () => invoke<void>("hide_main"),
  closeMain: () => invoke<void>("close_main"),
};

export function isTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
