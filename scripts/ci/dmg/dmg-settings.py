# dmgbuild settings for Asterium-macos-universal.dmg (ADR 0003). make-dmg.sh passes -D app=<path> -D background=<png>.
# dmgbuild runs this file with `defines` in scope; icon positions match make-background.py.
import os.path

application = defines["app"]  # noqa: F821 - provided by dmgbuild
appname = os.path.basename(application)

format = "UDZO"
filesystem = "HFS+"
files = [application]
symlinks = {"Applications": "/Applications"}
hide_extension = [appname]

background = defines["background"]  # noqa: F821
window_rect = ((200, 120), (640, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 96
text_size = 13
icon_locations = {appname: (150, 150), "Applications": (490, 150)}
