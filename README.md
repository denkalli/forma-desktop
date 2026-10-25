# Forma Desktop

**Forma** is a free and open-source graphic design application. Start from a template and
have something ready in minutes, or open the vector tools and design every detail from
scratch — in the same document.

- **Templates** — posts, posters, presentations, invitations, resumes and more. Swap the
  text and images, export, done. *(coming in the next phase)*
- **Infinite canvas with frames** — each format (a square post, a story, an A4 sheet, a
  slide) is a frame on the canvas; draw inside it and layers belong to it.
- **Vector editing** — shapes, pen tool with point editing, boolean operations, layers,
  gradients, alignment, distribution, rulers and smart guides.
- **Every template is editable** — a template is an ordinary document, editable down to
  the last node.
- **Text boxes** with automatic width, automatic height or a fixed size, and a font list
  with search and preview. Add your own font files from the list.
- **Export** frames, the selection or the whole page to PNG, SVG and PDF.
- **English and Portuguese** interface (Settings → General → Language).
- Optional local MCP server (Settings → Project) for clients to inspect and edit the open design.

## Local-first

Forma runs entirely on your computer:

- No account, no sign-in, no internet connection required.
- Add the folders where you keep your designs; the start screen shows each one with
  thumbnails. Designs are plain `.forma` files: move them, back them up or sync them with
  any tool you like. Removing a folder from the list never deletes anything.
- Designs that already have a file are saved automatically shortly after each change.
- No telemetry, no ads, no paywalls, no watermarks. Forma is non-profit.
- Fonts you add and your own templates live in Forma's data folder
  (`~/.local/share/forma/fonts` and `.../templates`; in the Flatpak,
  `~/.var/app/io.github.denkalli.Forma/data/forma/`; on Windows,
  `%APPDATA%\forma\fonts` and `...\templates`). Any `.forma` file in the templates folder
  shows up on the Templates page. Settings are in `settings.json` in the config folder
  (`~/.config/forma/`, or `%APPDATA%\forma\` on Windows).

> **Status:** early development. The first release (v0.1.0) targets Linux only; Windows
> builds are in progress, and macOS will follow.

## Building and running

Requires Rust 1.89 or newer and a GPU with Vulkan, Metal or Direct3D 12 support.

Forma Desktop depends on [libforma](https://github.com/denkalli/libforma) by path, so clone
both repositories next to each other:

```sh
git clone https://github.com/denkalli/libforma
git clone https://github.com/denkalli/forma-desktop
cd forma-desktop
```

```sh
cargo run --release                      # start screen
cargo run --release -- path/to/folder    # add a project folder
cargo run --release -- design.forma      # open a design
```

Forma Desktop is built on **libforma**, which holds the document model, rendering and file
format. During development both live side by side (`../libforma`).

### Flatpak

`packaging/flatpak/` holds the Flatpak manifest (app ID `io.github.denkalli.Forma`), the
desktop entry, AppStream metadata, MIME type and icon. To build and install it for the
current user:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.flatpak.Builder org.freedesktop.Sdk//26.08 \
    org.freedesktop.Sdk.Extension.rust-stable//26.08 org.freedesktop.Platform//26.08
packaging/flatpak/build.sh
flatpak run io.github.denkalli.Forma
```

`build.sh` vendors the Rust dependencies first, so the build itself runs offline. The app
can read and write your home folder, where project folders usually live. Inside the
sandbox, generic font families such as `sans-serif` resolve through the runtime's font
configuration, so text set in them may use a different font than the native build.

### Windows

Building on Windows needs the MSVC toolchain (Visual Studio Build Tools with the "Desktop
development with C++" workload) and a GPU with Direct3D 12 or Vulkan support.

`packaging/windows/build.ps1` builds a release and writes to `target\windows\`:

- `Forma-<version>-windows-x64.zip`, a portable folder: unzip it anywhere and run
  `forma.exe`.
- `Forma-<version>-windows-x64-setup.exe`, an installer made with
  [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`),
  when it is installed. The installer works without administrator rights, adds Forma to the
  Start menu and opens `.forma` files with Forma. Pass `-ZipOnly` to skip it.

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
```

Templates installed with Forma go in a `templates` folder next to `forma.exe`. The builds are
not code-signed, so Windows SmartScreen may warn the first time: choose **More info → Run
anyway**.

`assets/icon-256.png` (the window icon) and `packaging/windows/forma.ico` (embedded in
`forma.exe` by `build.rs`) are rendered from the SVG icon by `packaging/windows/make-icons.ps1`;
run it again (it needs `cargo install resvg`) if the icon changes.

## Keyboard shortcuts

| Action | Keys |
|---|---|
| Select, Frame, Rectangle, Ellipse, Line, Pen, Text | V, F, R, O, L, P, T |
| Undo / Redo | Ctrl+Z / Ctrl+Shift+Z |
| Save / Save as | Ctrl+S / Ctrl+Shift+S |
| New design / Add project folder | Ctrl+N / Ctrl+O |
| Export | Ctrl+E |
| Copy, cut, paste, duplicate | Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+D |
| Group / Ungroup | Ctrl+G / Ctrl+Shift+G |
| Bring forward / Send backward | Ctrl+] / Ctrl+[ |
| Distribute horizontally / vertically | Alt+Shift+H / Alt+Shift+V |
| Show or hide rulers (also in the canvas right-click menu) | Shift+R |
| Move selection | Arrows (Shift: 10 px) |
| Pan | Scroll, middle button, or Space+drag |
| Zoom | Ctrl+scroll, Ctrl+= / Ctrl+− |
| Zoom to fit / 100 % | Shift+0 / Shift+1 |
| Finish path, leave editing | Enter, Esc |

While drawing: Shift keeps proportions (and snaps lines to 45°), Alt draws from the centre.
While moving: Shift locks the axis, Ctrl disables snapping. Double-click enters groups,
edits text, or edits the points of a shape. Clicking inside a frame acts on its layers;
drag a frame by its name.

## Development aids

- `FORMA_THEME=light|dark` and `FORMA_LANG=en|pt` override the theme and the language
  without changing the saved settings.
- `FORMA_SCREENSHOT=out.png` saves a screenshot of the window after a few frames and quits;
  `FORMA_SCREENSHOT_SELECT=<layer name>` selects a layer first, and
  `FORMA_SCREENSHOT_OPEN=new|export|files|fonts|settings|settings-project|templates` opens
  a dialog, a list or a page first.
- `cargo test --release perf -- --ignored --nocapture` times editing a page with 5 500
  layers.

## Model Context Protocol (MCP)

The local MCP server is disabled by default. Enable **Settings → Project → MCP server** to
allow a compatible client to access the open design. The endpoint is
`http://127.0.0.1:43121/mcp`; send the displayed token in the
`Authorization: Bearer <token>` header. The server binds only to loopback, validates the
HTTP host and origin, and stops when disabled or when Forma exits. Editing tools create
ordinary undoable document changes. Keep the token private.

The server provides `read_document`, `list_pages`, `list_layers`, `create_rectangle`,
`rename_layer`, `transform_layer`, `move_layer` and `delete_layer` tools. Layer changes are
applied to the design currently open in Forma.

After enabling the server in Forma, copy its bearer token from **Settings → Project** and
set it in the `FORMA_MCP_TOKEN` environment variable before launching the client.

### Claude Code

Add this to `.mcp.json` in the project root. Claude Code expands the environment variable in
HTTP headers, so the token does not need to be stored in the JSON file:

```json
{
  "mcpServers": {
    "forma": {
      "type": "http",
      "url": "http://127.0.0.1:43121/mcp",
      "headers": {
        "Authorization": "Bearer ${FORMA_MCP_TOKEN}"
      }
    }
  }
}
```

### Codex

Add this to `~/.codex/config.toml`. Launch Codex from an environment where
`FORMA_MCP_TOKEN` is set:

```toml
[mcp_servers.forma]
url = "http://127.0.0.1:43121/mcp"
bearer_token_env_var = "FORMA_MCP_TOKEN"
```

The JSON example is for Claude Code. Claude Desktop connects to remote MCP servers through
**Settings → Connectors**; its `claude_desktop_config.json` is for local stdio servers and
cannot configure Forma's HTTP endpoint.

## Translations

Interface texts are written in English in the code, as `tr("…")` (or `trf("… {n}", …)` for
texts with values), and translated through the tables in `src/i18n/`. A test checks that
every text has a Portuguese translation with the same placeholders, kept in English
alphabetical order.

## Fonts and icons

The interface uses [Inter](https://rsms.me/inter/) (SIL Open Font License, see
`assets/fonts/Inter-OFL.txt`) and [Lucide](https://lucide.dev) icons (ISC License).

## Contributing

Contributions are welcome. Please open an issue to discuss larger changes before
sending a pull request.

## License

Forma is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version.

See [`LICENSE`](LICENSE) for the full text.
