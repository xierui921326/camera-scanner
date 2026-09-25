import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Device, ScanFinished, ScanOptions, ScanProgress } from "./types";

export function scanStart(opts: ScanOptions): Promise<number> {
  return invoke<number>("scan_start", { opts });
}

export function scanCancel(taskId: number): Promise<void> {
  return invoke("scan_cancel", { taskId });
}

export function deviceList(taskId: number): Promise<Device[]> {
  return invoke<Device[]>("device_list", { taskId });
}

export function whitelistList(): Promise<string[]> {
  return invoke<string[]>("whitelist_list");
}

export function whitelistAdd(target: string, confirm: boolean): Promise<string[]> {
  return invoke<string[]>("whitelist_add", { target, confirm });
}

export function whitelistRemove(target: string): Promise<string[]> {
  return invoke<string[]>("whitelist_remove", { target });
}

export function resultsExport(taskId: number, fmt: "csv" | "json"): Promise<string> {
  return invoke<string>("results_export", { taskId, fmt });
}

export function onScanDevice(cb: (device: Device) => void): Promise<UnlistenFn> {
  return listen<Device>("scan://device", (e) => cb(e.payload));
}

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan://progress", (e) => cb(e.payload));
}

export function onScanFinished(cb: (f: ScanFinished) => void): Promise<UnlistenFn> {
  return listen<ScanFinished>("scan://finished", (e) => cb(e.payload));
}
