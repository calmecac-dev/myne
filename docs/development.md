# Development

## Building

Debian, Ubuntu:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libpoppler-glib-dev blueprint-compiler groff
```

```bash
cargo build
```

`build.rs` runs `blueprint-compiler` on each file in `data/ui/`, then bundles
the generated `.ui` files, the icons and `data/style.css` into a GResource
embedded in the binary. Nothing under `data/` is read from disk at run time.

If the system `blueprint-compiler` does not run, point the build at another
one. On Ubuntu 24.04 this happens when `python3` has been switched to a
version without PyGObject; a two-line wrapper that calls
`/usr/bin/python3.12 /usr/bin/blueprint-compiler "$@"` is enough:

```bash
BLUEPRINT_COMPILER=/path/to/blueprint-compiler cargo build
```

## Checks

Every change is expected to pass all three:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Rules the code follows

- **The Blueprint files are the source of truth for the interface.** Widgets
  are looked up by the `id` they have there, through `#[template_child]`; the
  window is not rebuilt in code.
- **Nothing slow runs on the main loop.** Files are read and written with the
  `_future` methods of gio, and `pdfmom` runs through `gio::Subprocess`, driven
  by `glib::spawn_future_local`. Myne starts no threads of its own.
- **No `unwrap()` or `expect()` on interface paths.** Errors go to the log or
  to a dialog. The only `expect()` calls are on constant regular expressions,
  which the tests exercise.
- **`compiler.rs` depends on glib and gio only**, so its parser is tested
  without a display. Its test inputs are real output from groff 1.23, except
  the one for the older `troff: file:line:` format. Add a test with the exact
  line whenever a new message format turns up.
- **Every source file starts with an SPDX line:**
  `// SPDX-License-Identifier: GPL-3.0-or-later`, or the equivalent comment for
  its format.
- **Code, identifiers, commits and documentation are in English.** The
  interface is in Spanish.

## Where things live

| File | What it does |
|------|--------------|
| `src/application.rs` | Application actions, keyboard shortcuts, About |
| `src/window.rs` | Opening and saving, starting and finishing compilations, status, banner |
| `src/compiler.rs` | The `pdfmom` command line, the child process, cancelling, parsing messages |
| `src/viewer.rs` | `MynePdfView`: layout, zoom, keeping the position, rendering and caching pages |
| `src/log_panel.rs` | The rows of the compilation log |
| `src/preferences.rs` | The Preferences window, bound to `Config` |
| `src/config.rs` | `~/.config/myne/config.toml` |
| `src/editor.rs` | Small helpers around the GtkSourceView buffer |

## Testing by hand

There is no automated interface test. These are the checks that matter after
touching compilation or the viewer:

1. Open a `.mom` file, edit it and press `Ctrl+S`: the PDF appears.
2. Add `.so missing.mom` and save: the log opens, the banner shows, and the
   previous PDF stays.
3. Press `Ctrl+S` many times quickly: `ps` shows at most one `pdfmom` process
   group, and none a moment later.
4. Scroll to a later page, fix the error and save: the page and position stay.
5. Start Myne with a `PATH` that has no `pdfmom`: it starts and explains what
   is missing.
