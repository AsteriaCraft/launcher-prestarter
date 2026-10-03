import { describe, expect, it } from "vitest";
import { canPlayNow, initialState, reduce, type ViewEvent, type ViewState } from "./state";

const run = (events: ViewEvent[], from: ViewState = initialState) => events.reduce(reduce, from);

describe("view state", () => {
  it("resets the counters when the stage changes", () => {
    const state = run([
      { type: "stage", stage: "jreDownload" },
      { type: "progress", done: 50, total: 100, at: 0 },
      { type: "stage", stage: "jreInstall" },
    ]);
    expect(state.stage).toBe("jreInstall");
    expect(state.done).toBe(0);
    expect(state.total).toBe(0);
    expect(state.speed).toBeNull();
  });

  it("keeps the counters when the same stage is reported again", () => {
    const before = run([{ type: "stage", stage: "jreDownload" }, { type: "progress", done: 10, total: 100, at: 0 }]);
    expect(reduce(before, { type: "stage", stage: "jreDownload" })).toBe(before);
  });

  it("measures the speed over the recent window only", () => {
    const state = run([
      { type: "stage", stage: "jreDownload" },
      { type: "progress", done: 0, total: 10_000_000, at: 0 },
      { type: "progress", done: 1_000_000, total: 10_000_000, at: 400 },
    ]);
    expect(state.speed).toBeNull();
    const later = run(
      [
        { type: "progress", done: 2_000_000, total: 10_000_000, at: 1000 },
        { type: "progress", done: 9_000_000, total: 10_000_000, at: 3500 },
      ],
      state,
    );
    // The samples at 0 and 400 ms are older than 3 s: (9 MB - 2 MB) / 2.5 s.
    expect(later.speed).toBeCloseTo(2_800_000, 0);
    expect(later.samples.map((s) => s.at)).toEqual([1000, 3500]);
  });

  it("shows a failure and clears it on retry", () => {
    const failure = { message: "x", code: 4, retryable: true, command: null, logTail: null, link: null };
    const failed = run([{ type: "stage", stage: "jarDownload" }, { type: "failure", failure }]);
    expect(failed.failure).toEqual(failure);
    const retried = reduce(failed, { type: "retry" });
    expect(retried.failure).toBeNull();
    expect(retried.stage).toBe("preparing");
  });

  it("keeps one notice per kind", () => {
    const state = run([
      { type: "notice", notice: { kind: "testMode" } },
      { type: "notice", notice: { kind: "testMode" } },
      { type: "notice", notice: { kind: "newerWrapper", version: "0.4.0", page: "https://asterium.pro/launcher" } },
    ]);
    expect(state.notices.map((n) => n.kind)).toEqual(["testMode", "newerWrapper"]);
  });

  it("offers Play now only during a JRE update and only once", () => {
    const updating = run([{ type: "notice", notice: { kind: "jreUpdate" } }, { type: "stage", stage: "jreDownload" }]);
    expect(canPlayNow(updating)).toBe(true);
    expect(canPlayNow(reduce(updating, { type: "playNow" }))).toBe(false);
    expect(canPlayNow(reduce(updating, { type: "stage", stage: "launching" }))).toBe(false);
    expect(canPlayNow(run([{ type: "stage", stage: "jreDownload" }]))).toBe(false);
  });

  it("finishes on done", () => {
    const state = run([{ type: "stage", stage: "launching" }, { type: "done" }]);
    expect(state.finished).toBe(true);
    expect(state.stage).toBe("done");
  });
});
