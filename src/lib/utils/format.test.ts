import { describe, expect, it } from "vitest";
import { translator } from "$lib/i18n";
import { formatBytes, formatSpeed, percent } from "./format";

describe("formatting", () => {
  it("writes sizes like the Rust side", () => {
    expect(formatBytes(120_037_237, translator("uk"))).toBe("120 МБ");
    expect(formatBytes(8_503_901, translator("en"))).toBe("8.5 MB");
    expect(formatBytes(8_503_901, translator("uk"))).toBe("8,5 МБ");
    expect(formatBytes(420, translator("en"))).toBe("420 B");
    expect(formatBytes(1_500_000_000, translator("pl"))).toBe("1,5 GB");
  });

  it("writes speeds per second", () => {
    expect(formatSpeed(12_300_000, translator("uk"))).toBe("12,3 МБ/с");
    expect(formatSpeed(12_300_000, translator("en"))).toBe("12.3 MB/s");
  });

  it("clamps percentages", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(300, 200)).toBe(100);
    expect(percent(1, 0)).toBe(0);
    expect(percent(Number.NaN, 10)).toBe(0);
  });
});
