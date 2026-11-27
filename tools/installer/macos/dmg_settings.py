# dmgbuild's settings for the macOS disk image (build-macos.sh):
#   dmgbuild -s dmg_settings.py -D app=<.app> -D icon=<.icns> -D background=<dmg-background.png> \
#     "Mister Download Manager" out.dmg
# The standard installer window: no toolbar or sidebar, the app on the left and Applications on
# the right at 128 pt, over dmg-background.png (its arrow and panel are drawn for these places:
# dmg_background.py). dmgbuild writes Finder's view settings (.DS_Store) itself, without Finder,
# so this works on a build machine with no one logged in.
import os.path

application = defines["app"]  # noqa: F821
app_name = os.path.basename(application)

# LZMA (ULMO, macOS 10.15 and later; the app needs 11) packs it tighter than zlib (UDZO).
format = "ULMO"
filesystem = "HFS+"
size = None  # as small as its files

files = [application]
symlinks = {"Applications": "/Applications"}
# The volume's own icon in Finder and on the desktop: the app's.
icon = defines.get("icon")  # noqa: F821

# dmg-background.png and dmg-background@2x.png: dmgbuild joins them into one TIFF for Retina.
background = defines["background"]  # noqa: F821
window_rect = ((200, 120), (640, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False
show_item_info = False
arrange_by = None
label_pos = "bottom"
icon_size = 128
text_size = 13
icon_locations = {
    app_name: (170, 190),
    "Applications": (470, 190),
}
