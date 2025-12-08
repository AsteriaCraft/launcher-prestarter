export const appConfig = {
  name: "Asterium Craft",
  version: "1.0.0",
  download: {
    speedSamples: 30,
    initialDelay: 100,
  },
};

export const tauriCommands = {
  startDownload: "start_download",
  closeApp: "close_app",
};

export const tauriEvents = {
  downloadProgress: "download-progress",
  extractProgress: "extract-progress",
  error: "error",
  running: "running",
  done: "done",
};
