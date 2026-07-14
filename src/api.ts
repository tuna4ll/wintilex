// Typed wrappers over the Tauri commands in src-tauri/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";

export type LayoutKind = "bsp" | "columns" | "rows" | "main-stack" | "monocle";
export type RuleAction = "tile" | "float" | "ignore";
export type HotkeyBackend = "hook" | "system";
export type Direction = "left" | "down" | "up" | "right";

export interface General {
  "tiling-enabled": boolean;
  "start-on-login": boolean;
  "minimize-to-tray": boolean;
  "focus-follows-mouse": boolean;
  "warp-cursor-to-focus": boolean;
  "absorb-manual-resize": boolean;
  "hotkey-backend": HotkeyBackend;
  "protect-system-shortcuts": boolean;
}

export interface WindowRule {
  process: string | null;
  class: string | null;
  "title-contains": string | null;
  action: RuleAction;
  monitor: number | null;
  disabled: boolean;
}

/** Serialised form of `tilex_core::command::Action`. */
export interface Action {
  kind: string;
  value?: unknown;
}

export interface Hotkey {
  binding: string;
  kind: string;
  value?: unknown;
  disabled: boolean;
}

export interface Config {
  general: General;
  layout: LayoutKind;
  gap: number;
  "outer-gap": number;
  "main-ratio": number;
  reversed: boolean;
  hotkeys: Hotkey[];
  rules: WindowRule[];
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ManagedWindow {
  id: string;
  title: string;
  class: string;
  process: string;
  floating: boolean;
  minimized: boolean;
  monitor: string;
  tile: Rect | null;
}

export interface MonitorView {
  id: string;
  workArea: Rect;
  isPrimary: boolean;
  layout: LayoutKind;
  tiledWindows: number;
}

export interface Snapshot {
  tilingEnabled: boolean;
  windows: ManagedWindow[];
  monitors: MonitorView[];
  focused: string | null;
}

export interface HotkeyIssue {
  binding: string;
  reason: string;
}

export interface Environment {
  version: string;
  configPath: string;
  layouts: { id: LayoutKind; label: string }[];
  autostartEnabled: boolean;
}

export const getConfig = () => invoke<Config>("get_config");
export const saveConfig = (config: Config) => invoke<void>("save_config", { config });
export const resetConfig = () => invoke<Config>("reset_config");
export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const getHotkeyIssues = () => invoke<HotkeyIssue[]>("get_hotkey_issues");
export const getEnvironment = () => invoke<Environment>("get_environment");
export const revealConfig = () => invoke<void>("reveal_config");
export const runAction = (action: Action) => invoke<void>("run_action", { action });
