// What the window shows, as a pure reducer over the events from Rust (tested without a webview).

import { ui } from "$lib/config/app";
import type { FailurePayload, Notice, Stage } from "$lib/types/events";

export interface ViewState {
  stage: Stage;
  done: number;
  total: number;
  /** Bytes per second over the last few seconds; null until there is enough data. */
  speed: number | null;
  samples: Array<{ at: number; done: number }>;
  notices: Notice[];
  failure: FailurePayload | null;
  finished: boolean;
  playNowRequested: boolean;
}

export type ViewEvent =
  | { type: "stage"; stage: Stage }
  | { type: "progress"; done: number; total: number; at: number }
  | { type: "notice"; notice: Notice }
  | { type: "failure"; failure: FailurePayload }
  | { type: "done" }
  | { type: "retry" }
  | { type: "playNow" };

export const initialState: ViewState = {
  stage: "preparing",
  done: 0,
  total: 0,
  speed: null,
  samples: [],
  notices: [],
  failure: null,
  finished: false,
  playNowRequested: false,
};

/** Stages with a byte count; the others show an indeterminate bar. */
export const measuredStages: ReadonlySet<Stage> = new Set<Stage>(["jreDownload", "jreInstall", "jarDownload"]);

/** Stages where the speed matters (downloads). */
export const downloadStages: ReadonlySet<Stage> = new Set<Stage>(["jreDownload", "jarDownload"]);

const MIN_SPEED_SPAN_MS = 500;

export function reduce(state: ViewState, event: ViewEvent): ViewState {
  switch (event.type) {
    case "stage":
      if (event.stage === state.stage) return state;
      return { ...state, stage: event.stage, done: 0, total: 0, speed: null, samples: [] };
    case "progress": {
      const samples = [...state.samples, { at: event.at, done: event.done }].filter(
        (s) => event.at - s.at <= ui.speedWindowMs,
      );
      const first = samples[0];
      const span = event.at - first.at;
      const speed = span >= MIN_SPEED_SPAN_MS ? ((event.done - first.done) * 1000) / span : state.speed;
      return { ...state, done: event.done, total: event.total, samples, speed };
    }
    case "notice":
      if (state.notices.some((n) => n.kind === event.notice.kind)) return state;
      return { ...state, notices: [...state.notices, event.notice] };
    case "failure":
      return { ...state, failure: event.failure, finished: false };
    case "done":
      return { ...state, stage: "done", finished: true, failure: null };
    case "retry":
      return { ...state, failure: null, finished: false, stage: "preparing", done: 0, total: 0, speed: null, samples: [] };
    case "playNow":
      return { ...state, playNowRequested: true };
  }
}

/** "Play now" is offered while a JRE update downloads or unpacks, until it is pressed. */
export function canPlayNow(state: ViewState): boolean {
  return (
    !state.playNowRequested &&
    state.failure === null &&
    state.notices.some((n) => n.kind === "jreUpdate") &&
    (state.stage === "jreDownload" || state.stage === "jreInstall" || state.stage === "jreCheck")
  );
}
