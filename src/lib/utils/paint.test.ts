import { describe, expect, it } from "vitest";
import { whenPaintable, type PaintDeps } from "./paint";

const never = () => new Promise<never>(() => {});

function deps(overrides: Partial<PaintDeps> = {}): PaintDeps & { fonts: string[]; images: string[] } {
  const fonts: string[] = [];
  const images: string[] = [];
  return {
    fonts,
    images,
    loadFont: (font) => (fonts.push(font), Promise.resolve()),
    loadImage: (src) => (images.push(src), Promise.resolve()),
    delay: never,
    ...overrides,
  };
}

describe("showing the hidden window", () => {
  it("waits for every font and image the first frame paints", async () => {
    const d = deps();
    await expect(whenPaintable(['700 16px "Inter"', '16px "Kharkiv"'], ["/back.jpg"], 1500, d)).resolves.toBe(true);
    expect(d.fonts).toEqual(['700 16px "Inter"', '16px "Kharkiv"']);
    expect(d.images).toEqual(["/back.jpg"]);
  });

  it("does not hold the window back for a resource that fails", async () => {
    const d = deps({ loadImage: () => Promise.reject(new Error("decode failed")) });
    await expect(whenPaintable(["16px Kharkiv"], ["/missing.jpg"], 1500, d)).resolves.toBe(true);
  });

  it("gives up after the timeout when a resource never loads", async () => {
    let waited = 0;
    const d = deps({
      loadFont: never,
      delay: (ms) => ((waited = ms), Promise.resolve()),
    });
    await expect(whenPaintable(["16px Kharkiv"], [], 1500, d)).resolves.toBe(false);
    expect(waited).toBe(1500);
  });

  it("shows at once when there is nothing to wait for", async () => {
    await expect(whenPaintable([], [], 1500, deps())).resolves.toBe(true);
  });
});
