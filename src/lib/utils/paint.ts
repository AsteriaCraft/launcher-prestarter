// When the hidden window may be shown (crossplatform.md 4.4). WebKit (Linux, macOS) runs no requestAnimationFrame in
// a hidden window, so the page cannot wait for a painted frame there: it waits for what its first frame needs (the
// fonts and the background) and then asks Rust to show the window. A timeout keeps a resource that never loads from
// holding the window back.

export interface PaintDeps {
  loadFont: (font: string) => Promise<unknown>;
  loadImage: (src: string) => Promise<unknown>;
  delay: (ms: number) => Promise<void>;
}

export const browserDeps = (): PaintDeps => ({
  loadFont: (font) => document.fonts.load(font),
  loadImage: (src) => {
    const image = new Image();
    image.src = src;
    return image.decode();
  },
  delay: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
});

/**
 * Resolves `true` once every font and image has loaded or failed, `false` if `timeoutMs` passes first.
 * Fonts use the CSS `font` shorthand, e.g. `700 16px "Inter"`.
 */
export function whenPaintable(
  fonts: readonly string[],
  images: readonly string[],
  timeoutMs: number,
  deps: PaintDeps = browserDeps(),
): Promise<boolean> {
  const loads = Promise.allSettled([
    ...fonts.map((font) => deps.loadFont(font)),
    ...images.map((src) => deps.loadImage(src)),
  ]).then(() => true);
  return Promise.race([loads, deps.delay(timeoutMs).then(() => false)]);
}
