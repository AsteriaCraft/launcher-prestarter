// Payloads of the events and commands shared with Rust (src-tauri/src/app/events.rs, app/commands.rs).

export type Lang = "be" | "en" | "pl" | "ru" | "uk";

export type Stage =
  | "preparing"
  | "waitLock"
  | "jreCheck"
  | "jreDownload"
  | "jreInstall"
  | "jreVerify"
  | "jarDownload"
  | "launching"
  | "done";

export interface Boot {
  lang: Lang;
  testMode: boolean;
  os: "windows" | "linux" | "macos";
}

export interface StagePayload {
  stage: Stage;
}

export interface ProgressPayload {
  done: number;
  total: number;
}

export type Notice =
  | { kind: "jreUpdate" }
  | { kind: "newerWrapper"; version: string; page: string }
  | { kind: "translocated" }
  | { kind: "testMode" };

export interface FailurePayload {
  message: string;
  code: number;
  retryable: boolean;
  command: string | null;
  logTail: string | null;
  link: string | null;
}
