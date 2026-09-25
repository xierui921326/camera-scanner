import { invoke } from "@tauri-apps/api/core";
import type { AuditResult, Device, StreamInfo, StreamOpenArgs } from "./types";

export function auditDevice(
  ip: string,
  ports: number[],
  tryWeakCreds: boolean,
  timeoutMs?: number,
): Promise<AuditResult> {
  return invoke<AuditResult>("audit_device", { ip, ports, tryWeakCreds, timeoutMs });
}

export function streamOpen(opts: StreamOpenArgs): Promise<StreamInfo> {
  return invoke<StreamInfo>("stream_open", { opts });
}

export function streamClose(sessionId: string): Promise<void> {
  return invoke("stream_close", { sessionId });
}

export function streamList(): Promise<StreamInfo[]> {
  return invoke<StreamInfo[]>("stream_list");
}

export function historyDevices(): Promise<Device[]> {
  return invoke<Device[]>("history_devices");
}
