# Changelog

This file records the published Forma Desktop releases. There are no published `v0.3.0`
or `v0.4.0` tags.

## 0.6.2 — 2026-10-25

### Fixed

- Create a text box on a completed canvas click so its editor does not compete with the
  pointer-down event.

## 0.6.1 — 2026-10-25

### Added

- Save a selection as an editable `.forma` document, choose its internal name, and optionally
  include a PNG thumbnail.
- Use white text by default in Forma Dark and black text by default in Forma Light.

### Fixed

- Keep the in-place text editor inside the canvas so it does not draw over or intercept the
  inspector.
- Use the canvas click sequence for double-click editing and release short pointer clicks
  reliably.
- Make text-box sizing controls reachable while in-place text editing is active.
- Keep long unbroken text inside a constrained text box by wrapping it between characters.

## 0.6.0 — 2026-10-25

### Added

- Add a local MCP server for clients that can create text, shapes, and frames; set fills and
  strokes; organize layers; and undo or redo edits.
- Add release automation for Linux Flatpak and Windows packages.

## 0.5.0 — 2026-10-24

### Added

- Add an optional local MCP server with bearer-token authentication and settings in the app.

## 0.2.0 — 2026-10-25

### Added

- Port the desktop app to Windows, with a portable ZIP, per-user installer, file association,
  window icon, and Windows-specific project-folder and template handling.
- Publish Windows packages alongside the Linux release.

## 0.1.0 — 2026-10-24

### Added

- Initial Linux desktop release with an infinite canvas, frames, vector and text tools, layers,
  alignment, distribution, rulers, boolean operations, and PNG/SVG/PDF export.
- Add project folders, a templates page, English and Portuguese interface text, settings,
  Flatpak packaging, and `.forma` file association.
