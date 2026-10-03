# AppImage environment (A5)

`apprun-env.txt` is the environment the prestarter process sees inside an AppImage built by Tauri's bundler
(linuxdeploy AppRun + `linuxdeploy-plugin-gtk.sh` hook + the type 2 runtime's `APPDIR`, `APPIMAGE`, `ARGV0`, `OWD`).
Captured on 2026-10-02 by running the v0.2.0 AppImage's extracted `AppRun` with a stub binary that printed `env`
(Ubuntu 24.04 container), then renamed to a realistic mount point and with a player's own `HOME`, `DISPLAY`,
`WAYLAND_DISPLAY`, `LANG` and `XDG_RUNTIME_DIR` added. `launch::environment` must remove or clean every variable
that points into `$APPDIR` and keep the player's own ones; the CI smoke checks the same on the real AppImage.
