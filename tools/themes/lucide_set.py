"""Make the Liquid Glass theme's icon set from Lucide (https://lucide.dev, ISC).

    npm pack lucide-static && tar xzf lucide-static-*.tgz
    python tools/themes/lucide_set.py package/

writes crates/app/assets/themes/liquid-glass/icons/{16,24}/<need>.svg (and -symbolic.svg) and
the set's LICENSE and SOURCES.txt. Each need (crates/app/src/themeicons.rs NEEDS) is one Lucide
icon: the file types, categories and states in a colour of their own, read on light and dark
alike; the toolbar's and the rest one colour (white, which MDM tints with the text colour).
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "app" / "assets" / "themes" / "liquid-glass" / "icons"

BLUE, PURPLE, GREEN, RED = "#3b82f6", "#8b5cf6", "#22a35a", "#e5484d"
AMBER, TEAL, ORANGE, GREY = "#d18a00", "#14a3b0", "#e07a1f", "#8a8f98"

# need -> (Lucide icon, colour; None: one colour, tinted)
NEEDS = {
    "cat-all": ("download", BLUE),
    "cat-compressed": ("file-archive", AMBER),
    "cat-documents": ("file-text", BLUE),
    "cat-music": ("file-music", GREEN),
    "cat-programs": ("app-window", BLUE),
    "cat-video": ("file-video-camera", PURPLE),
    "cat-torrents": ("magnet", ORANGE),
    "cat-unfinished": ("circle-dashed", BLUE),
    "cat-finished": ("circle-check", GREEN),
    "cat-queues": ("list-ordered", BLUE),
    "cat-folder": ("folder", BLUE),
    "cat-folder-open": ("folder-open", BLUE),
    "ft-generic": ("file", GREY),
    "ft-archive": ("file-archive", AMBER),
    "ft-audio": ("file-music", GREEN),
    "ft-video": ("file-video-camera", PURPLE),
    "ft-image": ("file-image", TEAL),
    "ft-document": ("file-text", BLUE),
    "ft-pdf": ("file-text", RED),
    "ft-spreadsheet": ("file-spreadsheet", GREEN),
    "ft-presentation": ("presentation", ORANGE),
    "ft-text": ("file-text", GREY),
    "ft-code": ("file-code", PURPLE),
    "ft-executable": ("package", BLUE),
    "ft-disk-image": ("disc-3", GREY),
    "ft-font": ("file-type", GREY),
    "ft-torrent": ("magnet", ORANGE),
    "st-complete": ("circle-check", GREEN),
    "st-downloading": ("circle-arrow-down", BLUE),
    "st-paused": ("circle-pause", GREY),
    "st-queued": ("clock", GREY),
    "st-error": ("circle-alert", RED),
    "st-seeding": ("circle-arrow-up", GREEN),
    "misc-emblem-ok": ("circle-check", GREEN),
    "misc-up": ("chevron-up", None),
    "misc-down": ("chevron-down", None),
    "misc-remove": ("x", None),
    "tb-add-url": ("plus", None),
    "tb-add-torrent": ("magnet", None),
    "tb-resume": ("play", None),
    "tb-stop": ("pause", None),
    "tb-stop-all": ("octagon-pause", None),
    "tb-delete": ("trash-2", None),
    "tb-delete-completed": ("list-checks", None),
    "tb-options": ("settings", None),
    "tb-scheduler": ("alarm-clock", None),
    "tb-start-queue": ("circle-play", None),
    "tb-stop-queue": ("circle-stop", None),
    "ui-minimize": ("minus", None),
    "ui-maximize": ("square", None),
    "ui-restore": ("copy", None),
    "ui-close": ("x", None),
    "ui-to-tray": ("arrow-down-to-line", None),
}

# Lucide's 2-unit stroke on a 24-unit grid; a little lighter at 24 px, as SF Symbols' regular.
STROKE = {16: "2", 24: "1.75"}


def inner(svg: str) -> str:
    svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S)
    body = re.search(r"<svg[^>]*>(.*)</svg>", svg, re.S)
    if not body:
        raise ValueError("not an SVG")
    return re.sub(r"\s+", " ", body.group(1)).strip()


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    package = Path(sys.argv[1])
    icons = package / "icons"
    version = json.loads((package / "package.json").read_text())["version"]
    for px in STROKE:
        (OUT / str(px)).mkdir(parents=True, exist_ok=True)
        for old in (OUT / str(px)).glob("*.svg"):
            old.unlink()
    for need, (name, colour) in NEEDS.items():
        body = inner((icons / f"{name}.svg").read_text(encoding="utf-8"))
        file = f"{need}.svg" if colour else f"{need}-symbolic.svg"
        ink = colour or "#ffffff"
        for px, width in STROKE.items():
            (OUT / str(px) / file).write_text(
                f'<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" '
                f'viewBox="0 0 24 24" fill="none" stroke="{ink}" stroke-width="{width}" '
                f'stroke-linecap="round" stroke-linejoin="round">{body}</svg>\n',
                encoding="utf-8",
            )
    licence = package / "LICENSE"
    (OUT / "LICENSE").write_text(licence.read_text(encoding="utf-8"), encoding="utf-8")
    (OUT / "SOURCES.txt").write_text(
        "The Liquid Glass theme's icon set.\n\n"
        f"Lucide {version} (https://lucide.dev), ISC licence (LICENSE beside this file), made by\n"
        "tools/themes/lucide_set.py: each file is one Lucide icon, its strokes set in a colour\n"
        "(file types, categories, states) or in white, which MDM tints with the theme's text\n"
        "colour (`-symbolic`). Which icon each need is: NEEDS in that script.\n",
        encoding="utf-8",
    )
    print(f"{len(NEEDS)} icons from Lucide {version} -> {OUT}")


if __name__ == "__main__":
    main()
