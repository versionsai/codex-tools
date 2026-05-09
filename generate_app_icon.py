from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parent
ICONSET = ROOT / "build" / "Codex Sync.iconset"

SIZES = [
    (16, "icon_16x16.png"),
    (32, "icon_16x16@2x.png"),
    (32, "icon_32x32.png"),
    (64, "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_128x128@2x.png"),
    (256, "icon_256x256.png"),
    (512, "icon_256x256@2x.png"),
    (512, "icon_512x512.png"),
    (1024, "icon_512x512@2x.png"),
]


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def vertical_gradient(size: int, top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    image = Image.new("RGBA", (size, size))
    draw = ImageDraw.Draw(image)
    for y in range(size):
        t = y / max(1, size - 1)
        color = tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3)) + (255,)
        draw.line((0, y, size, y), fill=color)
    return image


def draw_icon(size: int) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    inset = int(size * 0.06)
    inner = size - inset * 2
    radius = int(inner * 0.24)

    tile = vertical_gradient(inner, (24, 137, 255), (8, 78, 235))
    mask = rounded_mask(inner, radius)
    canvas.alpha_composite(tile, (inset, inset))
    alpha = Image.new("L", (size, size), 0)
    alpha.paste(mask, (inset, inset))
    canvas.putalpha(alpha)

    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    glow_draw.ellipse(
        (int(size * 0.20), int(size * 0.10), int(size * 0.88), int(size * 0.60)),
        fill=(255, 255, 255, 52),
    )
    glow = glow.filter(ImageFilter.GaussianBlur(radius=max(2, int(size * 0.05))))
    canvas.alpha_composite(glow)

    draw = ImageDraw.Draw(canvas)
    outline = (255, 255, 255, 42)
    draw.rounded_rectangle(
        (inset, inset, size - inset - 1, size - inset - 1),
        radius=radius,
        outline=outline,
        width=max(1, int(size * 0.01)),
    )

    ring_box = (
        int(size * 0.27),
        int(size * 0.23),
        int(size * 0.73),
        int(size * 0.69),
    )
    ring_w = max(2, int(size * 0.065))
    draw.arc(ring_box, start=30, end=330, fill=(255, 255, 255, 240), width=ring_w)

    arrow = [
        (int(size * 0.65), int(size * 0.41)),
        (int(size * 0.54), int(size * 0.33)),
        (int(size * 0.54), int(size * 0.46)),
    ]
    draw.line([arrow[0], arrow[1]], fill=(255, 255, 255, 245), width=max(2, int(size * 0.04)))
    draw.line([arrow[1], arrow[2]], fill=(255, 255, 255, 245), width=max(2, int(size * 0.04)))

    pill = (
        int(size * 0.28),
        int(size * 0.66),
        int(size * 0.72),
        int(size * 0.77),
    )
    draw.rounded_rectangle(pill, radius=int(size * 0.05), fill=(255, 255, 255, 56))
    dot_r = max(2, int(size * 0.03))
    dot_y = int((pill[1] + pill[3]) / 2)
    for idx, alpha_value in enumerate((140, 240, 140)):
        dot_x = int(size * (0.39 + idx * 0.11))
        draw.ellipse((dot_x - dot_r, dot_y - dot_r, dot_x + dot_r, dot_y + dot_r), fill=(255, 255, 255, alpha_value))

    return canvas


def main() -> None:
    ICONSET.mkdir(parents=True, exist_ok=True)
    for size, filename in SIZES:
        image = draw_icon(size)
        image.save(ICONSET / filename)


if __name__ == "__main__":
    main()
