# Tauri migration

Python baseline: `76cf8b0`. Source in `src/freecut/` and `tests/` remains available
for comparison; new Windows releases use `frontend/` and `src-tauri/`.

Preserve:
- Freehand lasso and editable polygon; undo, move, reset, cancel and HUD toggle.
- Frozen monitor image, immediate capture, physical resolution, even-odd alpha mask.
- Transparent preview, native image + PNG clipboard and atomic PNG save.
- Configurable global shortcut, conflict rollback, saved settings.
- Tray, cursor monitor capture, background cancel, explicit quit, single instance.
- Existing Chinese UI, colors and keyboard controls. No AI or artificial delay.

Validation and measured release size will be recorded here after the build.
