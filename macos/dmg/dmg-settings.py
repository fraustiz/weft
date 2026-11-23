# dmgbuild settings for the Mac disk image: Weft.app on the left, a link to
# /Applications on the right, and the drawn arrow between them.
#
#     dmgbuild -s macos/dmg/dmg-settings.py \
#         -D app=macos/build/Weft.app -D background=background.tiff \
#         Weft Weft-X.Y.Z-mac.dmg
#
# Window size and icon centres must match make-background.py.
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
background = defines["background"]  # noqa: F821

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
# No hide_extensions: it sets Finder info on Weft.app, and
# codesign --verify --strict then rejects the bundle. Finder hides .app anyway.

# Finder counts the title bar in the window height (32 pt on macOS 26):
# 432 leaves 400 pt of content, the height of the background.
window_rect = ((200, 120), (640, 432))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 128
text_size = 13
icon_locations = {
    os.path.basename(app): (170, 180),
    "Applications": (470, 180),
}
