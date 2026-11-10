# Roadmap

What each release holds. Nothing has been released yet; 0.1.0 is what `main`
builds today.

## 0.1.0 — Editor, preview and log

Done.

- The main window from `data/ui/window.blp`: editor, PDF preview and a
  collapsible compilation log
- Open, save and save as, with a prompt before unsaved changes are lost
- Compilation with `pdfmom` on save and with `F5`, one at a time, cancelling
  the previous run when a new save arrives
- The previous PDF stays when a compilation fails
- PDF preview with fit to width, zoom, and the page and position kept across
  reloads
- groff's messages in the log as they arrive, by severity
- A check for `pdfmom` at startup, with an explanation when it is missing
- Preferences: compile on save, the command, and the tbl, eqn, pic and refer
  preprocessors

## 0.2.0 — Writing comfort

- Syntax highlighting for mom, with a GtkSourceView language file: macro lines
  starting with `.`, the escapes `\*[...]` and `\[...]`, and the comments `\"`
  and `.\"`
- An editor color scheme that follows the system's light or dark style; today
  the editor stays light in dark mode
- Log messages that take you to their line in the editor and mark it
- Completion of mom macros
- Recent files, and the size of the window and its panes remembered between
  sessions

## 0.3.0 — Working from the PDF

- Selecting and copying text in the PDF. `poppler-rs` 0.26 does not expose
  `Page::text_layout`, so this needs either a newer binding or a call into
  `poppler-glib` directly.
- Clicking in the PDF to jump to the source. The first version searches the
  source for the text under the click. An exact mapping, with `\X'...'` markers
  in groff's intermediate output, is research and not promised.
- Recompiling when the file changes on disk outside Myne.
