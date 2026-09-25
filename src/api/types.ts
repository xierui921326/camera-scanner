export interface Device {
  ip: string;
  openPorts: number[];
  protocols: string[];
  source: string;
  onvifXaddr: string | null;
  httpServer: string | null;
  discoveredAt: number;
}

export interface ScanOptions {
  cidr: string;
  ports: number[];
  rateLimit: number;
  timeoutMs: number;
  onvif: boolean;
  mdns: boolean;
}

export interface StreamOpenArgs {
  rtspUrl?: string;
  ip?: string;
  port?: number;
  path?: string;
  username?: string;
  password?: string;
  transcode: boolean;
  ffmpegPath?: string;
  mode?: "hls" | "mse" | "retina";
  label?: string;
}

export interface StreamInfo {
  sessionId: string;
  url: string;
  mode: string;
  label: string;
  manifestUrl: string | null;
}

export interface Finding {
  id: string;
  cve: string;
  title: string;
  severity: string;
  description: string;
  advice: string;
}

export interface WeakCred {
  scheme: string;
  username: string;
  password: string;
  path: string;
}

export interface AuditResult {
  ip: string;
  httpPort: number | null;
  vendor: string | null;
  realm: string | null;
  httpServer: string | null;
  findings: Finding[];
  weakCredentials: WeakCred[];
  notes: string[];
}

export interface ScanProgress {
  taskId: number;
  scanned: number;
  total: number;
}

export interface ScanFinished {
  taskId: number;
  found: number;
  cancelled: boolean;
}
