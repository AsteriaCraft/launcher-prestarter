// Names shared with Rust: commands (src-tauri/src/app/commands.rs) and events (src-tauri/src/app/events.rs).

export const commands = {
  boot: "boot",
  ready: "ready",
  retry: "retry",
  playNow: "play_now",
  openLogs: "open_logs",
  openPage: "open_page",
  quit: "quit",
  minimize: "minimize",
} as const;

export const events = {
  stage: "stage",
  progress: "progress",
  notice: "notice",
  failure: "failure",
  done: "done",
} as const;

export const ui = {
  /** How often the tip under the progress bar changes. */
  tipIntervalMs: 6000,
  /** Window over which the download speed is averaged. */
  speedWindowMs: 3000,
} as const;
