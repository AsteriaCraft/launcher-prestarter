"""Draws the DMG window background (ADR 0003): an arrow from the app to Applications and the first-start steps in
Ukrainian and English. The PNGs are committed; run this only to change the picture.

    python scripts/ci/dmg/make-background.py --font <regular.ttf> --bold <bold.ttf>

Needs Pillow and a font with Cyrillic (Segoe UI on Windows, Noto Sans or DejaVu Sans elsewhere). Icon positions
must match dmg-settings.py (app at x=150, Applications at x=490, both at y=150; the window is 640x420).
"""

import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 640, 420
APP_X, APPS_X, ICON_Y = 150, 490, 150

STEPS = {
    "uk": [
        "1. Перетягніть Asterium у «Програми».",
        "2. Відкрийте його. Якщо macOS не дозволяє: Системні параметри →",
        "    Конфіденційність і безпека → «Усе одно відкрити».",
        "3. Підтвердьте паролем або Touch ID.",
    ],
    "en": [
        "1. Drag Asterium to Applications.",
        "2. Open it. If macOS blocks it: System Settings →",
        "    Privacy & Security → Open Anyway.",
        "3. Confirm with your password or Touch ID.",
    ],
}


def draw(scale: int, regular: str, bold: str) -> Image.Image:
    image = Image.new("RGB", (WIDTH * scale, HEIGHT * scale), (15, 20, 36))
    pixels = image.load()
    # A soft vertical gradient like the prestarter window.
    for y in range(HEIGHT * scale):
        shade = y / (HEIGHT * scale)
        color = (int(15 + 10 * shade), int(20 + 6 * shade), int(36 + 24 * shade))
        for x in range(WIDTH * scale):
            pixels[x, y] = color
    canvas = ImageDraw.Draw(image)
    s = scale

    # Arrow from the app icon to Applications.
    y = ICON_Y * s
    canvas.line([((APP_X + 70) * s, y), ((APPS_X - 80) * s, y)], fill=(155, 77, 255), width=4 * s)
    tip = (APPS_X - 66) * s
    canvas.polygon([(tip, y), (tip - 16 * s, y - 10 * s), (tip - 16 * s, y + 10 * s)], fill=(92, 200, 255))

    title = ImageFont.truetype(bold, 15 * s)
    text = ImageFont.truetype(regular, 12 * s)
    canvas.text((24 * s, 232 * s), "Перший запуск · First start", font=title, fill=(243, 244, 251))
    top = 258
    for lang in ("uk", "en"):
        for line in STEPS[lang]:
            canvas.text((24 * s, top * s), line, font=text, fill=(190, 198, 222))
            top += 17
        top += 6
    return image


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--font", default="C:/Windows/Fonts/segoeui.ttf")
    parser.add_argument("--bold", default="C:/Windows/Fonts/segoeuib.ttf")
    args = parser.parse_args()
    out = Path(__file__).resolve().parent
    draw(1, args.font, args.bold).save(out / "background.png", optimize=True)
    draw(2, args.font, args.bold).save(out / "background@2x.png", optimize=True)


if __name__ == "__main__":
    main()
