"""The background of the macOS disk image's window (build-macos.sh, dmg_settings.py).

Writes dmg-background.png (640x400) and dmg-background@2x.png (1280x800) beside this file:
Liquid Glass colours, a frosted panel behind the two icons and an arrow from the app (left) to
Applications (right). The colours stay in the middle of the range, so Finder's icon labels read
in Light mode (black) and Dark mode (white) alike. Run it again after changing the layout:
  python3 tools/installer/macos/dmg_background.py
Needs Pillow and Inter (falls back to DejaVu Sans).
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
W, H = 640, 400
# Where dmg_settings.py puts the icons' centres (1x points).
APP = (170, 190)
APPLICATIONS = (470, 190)
TITLE = "Mister Download Manager"
HINT = "Drag the app into Applications to install it"


def font(size: int, bold: bool) -> ImageFont.FreeTypeFont:
    names = (
        ["Inter-SemiBold.otf", "InterDisplay-SemiBold.otf", "DejaVuSans-Bold.ttf"]
        if bold
        else ["Inter-Regular.otf", "InterDisplay-Regular.otf", "DejaVuSans.ttf"]
    )
    for root in ("/usr/share/fonts", "/Library/Fonts", "/System/Library/Fonts"):
        for name in names:
            for found in Path(root).rglob(name):
                return ImageFont.truetype(str(found), size)
    return ImageFont.load_default(size)


def blob(size, centre, radius, colour):
    """A soft round patch of colour, transparent at its edge."""
    layer = Image.new("RGBA", size, colour + (0,))
    mask = Image.new("L", size, 0)
    d = ImageDraw.Draw(mask)
    x, y = centre
    d.ellipse((x - radius, y - radius, x + radius, y + radius), fill=255)
    layer.putalpha(mask.filter(ImageFilter.GaussianBlur(radius * 0.6)))
    return layer


def draw(scale: int) -> Image.Image:
    w, h = W * scale, H * scale
    s = lambda v: int(round(v * scale))  # noqa: E731
    img = Image.new("RGBA", (w, h), (112, 128, 206, 255))
    for centre, radius, colour in [
        ((0.05, 0.0), 0.75, (120, 170, 232)),
        ((1.0, 0.05), 0.6, (206, 150, 170)),
        ((0.55, 1.1), 0.7, (88, 98, 196)),
        ((0.95, 0.95), 0.45, (124, 112, 204)),
    ]:
        img = Image.alpha_composite(
            img, blob((w, h), (s(centre[0] * W), s(centre[1] * H)), s(radius * W), colour)
        )

    # The frosted panel behind the icons and their labels.
    panel = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    pd = ImageDraw.Draw(panel)
    pd.rounded_rectangle(
        (s(48), s(92), s(W - 48), s(312)),
        radius=s(28),
        fill=(255, 255, 255, 34),
        outline=(255, 255, 255, 70),
        width=max(1, s(1)),
    )
    img = Image.alpha_composite(img, panel)

    d = ImageDraw.Draw(img)
    # The arrow, from the app to Applications, between the two icons (128 pt wide).
    y = s(APP[1])
    x0, x1 = s(APP[0] + 88), s(APPLICATIONS[0] - 88)
    shaft = max(2, s(3))
    white = (255, 255, 255, 220)
    d.line((x0, y, x1 - s(6), y), fill=white, width=shaft)
    d.polygon([(x1, y), (x1 - s(16), y - s(11)), (x1 - s(16), y + s(11))], fill=white)

    # Words, with a faint shadow so they read on every colour of the gradient.
    def text(xy, words, f, fill):
        shadow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        ImageDraw.Draw(shadow).text(
            (xy[0], xy[1] + s(1)), words, font=f, fill=(20, 24, 60, 110), anchor="mm"
        )
        nonlocal_img[0] = Image.alpha_composite(
            nonlocal_img[0], shadow.filter(ImageFilter.GaussianBlur(s(1.5)))
        )
        ImageDraw.Draw(nonlocal_img[0]).text(xy, words, font=f, fill=fill, anchor="mm")

    nonlocal_img = [img]
    text((s(W / 2), s(50)), TITLE, font(s(22), True), (255, 255, 255, 255))
    text((s(W / 2), s(350)), HINT, font(s(14), False), (255, 255, 255, 235))
    return nonlocal_img[0].convert("RGB")


if __name__ == "__main__":
    for scale, name in ((1, "dmg-background.png"), (2, "dmg-background@2x.png")):
        out = draw(scale)
        # 72 and 144 dpi: Finder's tiffutil -cathidpicheck pairs the two by their resolution.
        out.save(HERE / name, dpi=(72 * scale, 72 * scale), optimize=True)
        print(HERE / name, out.size)
