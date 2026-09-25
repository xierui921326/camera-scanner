import { invoke } from "@tauri-apps/api/core";

export interface AuditLogEntry {
  id: number;
  ts: number;
  action: string;
  target: string | null;
  detail: string | null;
  ok: boolean;
}

export function consentStatus(): Promise<boolean> {
  return invoke<boolean>("consent_status");
}

export function consentAccept(): Promise<void> {
  return invoke("consent_accept");
}

export function auditLogList(limit = 200): Promise<AuditLogEntry[]> {
  return invoke<AuditLogEntry[]>("audit_log_list", { limit });
}

export function reportGenerate(fmt: "md" | "html"): Promise<string> {
  return invoke<string>("report_generate", { fmt });
}
