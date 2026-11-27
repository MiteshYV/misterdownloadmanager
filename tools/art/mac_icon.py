"""The macOS app icon in Apple's style (#mac-fork): the 1024 px canvas, a rounded square (a
superellipse, Apple's "continuous" corner) 824 px wide with Liquid Glass light, and a soft shadow.
  python3 tools/art/mac_icon.py a   -> art/macos/icon-a.png: the mascot (art/Mister Download Manager.png) on glass
  python3 tools/art/mac_icon.py b   -> art/macos/icon-b.png: a glass globe, a download arrow, a top hat
  python3 tools/art/mac_icon.py c   -> art/macos/icon-c.png: the mascot itself made of glass (the chosen one)
  python3 tools/art/mac_icon.py install -> C where it is used: art/macos/AppIcon.png (the Mac app's
    bundle icon, build-macos.sh), crates/app/assets/icons/app/mdm-256-mac.png (the app's picture of
    itself on macOS) and the browser add-on's icons (extension/icons, its whole rounded square:
    toolbars draw icons without Apple's margin)
Drawn at 2x and scaled down, so its edges are smooth. Needs Pillow.
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "art" / "macos"
S = 2  # supersampling
N = 1024 * S
BODY = 824 * S
OFF = (N - BODY) // 2


def squircle(size, n=5.0):
    """A superellipse mask: Apple's rounded square."""
    m = Image.new("L", (size, size), 0)
    a = size / 2
    pts = []
    for i in range(720):
        t = 2 * math.pi * i / 720
        c, s = math.cos(t), math.sin(t)
        x = a + a * math.copysign(abs(c) ** (2 / n), c)
        y = a + a * math.copysign(abs(s) ** (2 / n), s)
        pts.append((x, y))
    ImageDraw.Draw(m).polygon(pts, fill=255)
    return m


def gradient(size, top, bottom):
    g = Image.new("RGBA", (1, 256))
    for y in range(256):
        f = y / 255
        g.putpixel((0, y), tuple(int(top[i] + (bottom[i] - top[i]) * f) for i in range(3)) + (255,))
    return g.resize((size, size))


def radial(size, centre, radius, colour, alpha):
    layer = Image.new("RGBA", (size, size), colour + (0,))
    m = Image.new("L", (size, size), 0)
    x, y = centre
    ImageDraw.Draw(m).ellipse((x - radius, y - radius, x + radius, y + radius), fill=alpha)
    layer.putalpha(m.filter(ImageFilter.GaussianBlur(radius * 0.5)))
    return layer


def glass_body(top, bottom):
    """The rounded square: a gradient, a soft light at the top, a glass rim."""
    body = gradient(BODY, top, bottom)
    body = Image.alpha_composite(body, radial(BODY, (BODY * 0.3, BODY * 0.05), BODY * 0.6, (255, 255, 255), 120))
    body = Image.alpha_composite(body, radial(BODY, (BODY * 0.85, BODY * 1.0), BODY * 0.5, (40, 20, 140), 70))
    return body


def finish(body):
    """Mask the body, add its rim and the drop shadow, place it on the 1024 canvas."""
    mask = squircle(BODY)
    rim = Image.new("RGBA", (BODY, BODY), (255, 255, 255, 0))
    inner = mask.resize((BODY - 6 * S, BODY - 6 * S))
    ring = mask.copy()
    hole = Image.new("L", (BODY, BODY), 0)
    hole.paste(inner, (3 * S, 3 * S))
    ring = ImageChops.subtract(ring, hole)
    # brighter at the top: light on the glass's edge
    fade = gradient(BODY, (255, 255, 255), (90, 90, 90)).convert("L")
    rim.putalpha(ImageChops.multiply(ring, fade))
    body = Image.alpha_composite(body, rim)
    body.putalpha(ImageChops.multiply(body.getchannel("A"), mask))
    canvas = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    sm = Image.new("L", (N, N), 0)
    sm.paste(mask, (OFF, OFF + 12 * S))
    shadow.putalpha(sm.filter(ImageFilter.GaussianBlur(22 * S)).point(lambda v: int(v * 0.5)))
    canvas = Image.alpha_composite(canvas, shadow)
    canvas.alpha_composite(body, (OFF, OFF))
    return canvas.resize((1024, 1024), Image.LANCZOS)


def icon_a():
    body = glass_body((236, 244, 255), (188, 206, 246))
    art = Image.open(ROOT / "art" / "Mister Download Manager.png").convert("RGBA")
    k = int(BODY * 0.86)
    art = art.resize((k, k), Image.LANCZOS)
    # its own soft shadow on the glass
    sh = Image.new("RGBA", art.size, (20, 30, 80, 0))
    sh.putalpha(art.getchannel("A").filter(ImageFilter.GaussianBlur(14 * S)).point(lambda v: int(v * 0.35)))
    body.alpha_composite(sh, ((BODY - k) // 2, (BODY - k) // 2 + 14 * S))
    body.alpha_composite(art, ((BODY - k) // 2, (BODY - k) // 2))
    return finish(body)


def icon_b():
    body = glass_body((86, 170, 255), (24, 76, 230))
    d = ImageDraw.Draw(body)
    c = BODY / 2
    # the glass globe
    r = BODY * 0.34
    globe = Image.new("RGBA", (BODY, BODY), (0, 0, 0, 0))
    gd = ImageDraw.Draw(globe)
    gd.ellipse((c - r, c - r + BODY * 0.03, c + r, c + r + BODY * 0.03), fill=(255, 255, 255, 46), outline=(255, 255, 255, 150), width=int(5 * S))
    cy = c + BODY * 0.03
    w = int(4 * S)
    for f in (0.42, 0.78):  # meridians
        gd.ellipse((c - r * f, cy - r, c + r * f, cy + r), outline=(255, 255, 255, 90), width=w)
    gd.line((c, cy - r, c, cy + r), fill=(255, 255, 255, 90), width=w)
    for f in (-0.5, 0, 0.5):  # parallels
        y = cy + r * f
        half = r * math.sqrt(1 - f * f)
        gd.line((c - half, y, c + half, y), fill=(255, 255, 255, 90), width=w)
    body = Image.alpha_composite(body, globe)
    # the download arrow: white, bold, with a soft shadow
    arrow = Image.new("RGBA", (BODY, BODY), (0, 0, 0, 0))
    ad = ImageDraw.Draw(arrow)
    sw, top, mid, tip, hw = BODY * 0.075, cy - r * 0.62, cy + r * 0.12, cy + r * 0.62, BODY * 0.17
    ad.rounded_rectangle((c - sw, top, c + sw, mid + sw), radius=sw * 0.6, fill=(255, 255, 255, 255))
    ad.polygon([(c - hw, mid - sw * 0.2), (c + hw, mid - sw * 0.2), (c, tip)], fill=(255, 255, 255, 255))
    ad.rounded_rectangle((c - r * 0.62, tip + BODY * 0.035, c + r * 0.62, tip + BODY * 0.035 + sw * 0.9), radius=sw * 0.45, fill=(255, 255, 255, 235))
    shadow = Image.new("RGBA", (BODY, BODY), (10, 20, 80, 0))
    shadow.putalpha(arrow.getchannel("A").filter(ImageFilter.GaussianBlur(10 * S)).point(lambda v: int(v * 0.45)))
    body.alpha_composite(shadow, (0, int(8 * S)))
    body = Image.alpha_composite(body, arrow)
    # the top hat, tilted on the globe's top right: the mascot's nod
    hat = Image.new("RGBA", (BODY, BODY), (0, 0, 0, 0))
    hd = ImageDraw.Draw(hat)
    hx, hy = c + r * 0.55, cy - r * 0.95
    hd.rounded_rectangle((hx - 70 * S, hy - 120 * S, hx + 70 * S, hy + 10 * S), radius=18 * S, fill=(22, 26, 44, 255))
    hd.rectangle((hx - 70 * S, hy - 20 * S, hx + 70 * S, hy + 6 * S), fill=(120, 220, 120, 255))
    hd.rounded_rectangle((hx - 112 * S, hy + 4 * S, hx + 112 * S, hy + 30 * S), radius=13 * S, fill=(22, 26, 44, 255))
    hat = hat.rotate(-14, center=(hx, hy), resample=Image.BICUBIC)
    hs = Image.new("RGBA", (BODY, BODY), (0, 0, 30, 0))
    hs.putalpha(hat.getchannel("A").filter(ImageFilter.GaussianBlur(8 * S)).point(lambda v: int(v * 0.4)))
    body.alpha_composite(hs, (0, int(6 * S)))
    body = Image.alpha_composite(body, hat)
    return finish(body)


def glassify(art):
    """The artwork as Liquid Glass: a little see-through, a sheen over its top, a bright rim of light
    around each shape (stronger at the top), and a soft coloured glow under it."""
    a = art.getchannel("A")
    w, h = art.size
    out = art.copy()
    out.putalpha(a.point(lambda v: int(v * 0.94)))
    # the sheen: white, strong at the top, gone by the middle
    sheen = gradient(w, (255, 255, 255), (0, 0, 0)).convert("L").point(lambda v: max(0, int((v - 110) * 1.25)))
    sheen = ImageChops.multiply(sheen.resize((w, h)), a)
    white = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    white.putalpha(sheen.point(lambda v: int(v * 0.55)))
    out = Image.alpha_composite(out, white)
    # the rim: each shape's edge, a few pixels in, lit from above
    inner = a.filter(ImageFilter.MinFilter(9 * S // 2 * 2 + 1))
    edge = ImageChops.subtract(a, inner).filter(ImageFilter.GaussianBlur(1.5 * S))
    light = gradient(w, (255, 255, 255), (120, 120, 120)).convert("L").resize((w, h))
    rim = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    rim.putalpha(ImageChops.multiply(edge, light).point(lambda v: int(v * 0.85)))
    out = Image.alpha_composite(out, rim)
    # a faint inner shade at the bottom of each shape: depth
    shade = gradient(w, (0, 0, 0), (255, 255, 255)).convert("L").point(lambda v: max(0, int((v - 150) * 0.9))).resize((w, h))
    dark = Image.new("RGBA", (w, h), (10, 20, 60, 0))
    dark.putalpha(ImageChops.multiply(ImageChops.multiply(shade, a), Image.new("L", (w, h), 90)))
    out = Image.alpha_composite(out, dark)
    return out


def icon_c():
    body = glass_body((120, 186, 255), (52, 92, 226))
    art = Image.open(ROOT / "art" / "Mister Download Manager.png").convert("RGBA")
    k = int(BODY * 0.84)
    art = art.resize((k, k), Image.LANCZOS)
    x, y = (BODY - k) // 2, (BODY - k) // 2 - int(6 * S)
    # the glow under the glass shapes: the art's own colours, blurred, as light through glass
    glow = art.filter(ImageFilter.GaussianBlur(26 * S))
    glow.putalpha(glow.getchannel("A").point(lambda v: int(v * 0.55)))
    body.alpha_composite(glow, (x, y + int(22 * S)))
    sh = Image.new("RGBA", art.size, (10, 20, 70, 0))
    sh.putalpha(art.getchannel("A").filter(ImageFilter.GaussianBlur(10 * S)).point(lambda v: int(v * 0.35)))
    body.alpha_composite(sh, (x, y + int(12 * S)))
    body.alpha_composite(glassify(art), (x, y))
    return finish(body)


def install():
    OUT.mkdir(parents=True, exist_ok=True)
    icon = icon_c()
    icon.save(OUT / "AppIcon.png", optimize=True)
    icon.resize((256, 256), Image.LANCZOS).save(
        ROOT / "crates/app/assets/icons/app/mdm-256-mac.png", optimize=True
    )
    # The rounded square alone, a hair of room kept for its shadow-less edge.
    m = int(1024 * (100 - 6) / 1024)
    body = icon.crop((m, m, 1024 - m, 1024 - m))
    for size in (16, 32, 48, 128):
        body.resize((size, size), Image.LANCZOS).save(ROOT / f"extension/icons/icon-{size}.png", optimize=True)
    print("installed", OUT / "AppIcon.png")


if __name__ == "__main__":
    if sys.argv[1:] == ["install"]:
        install()
        sys.exit()
    OUT.mkdir(parents=True, exist_ok=True)
    for which in sys.argv[1:] or ["a", "b"]:
        img = {"a": icon_a, "b": icon_b, "c": icon_c}[which]()
        img.save(OUT / f"icon-{which}.png", optimize=True)
        print(OUT / f"icon-{which}.png")
