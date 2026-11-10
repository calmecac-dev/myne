# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Main window with an editor, a PDF preview and a collapsible compilation log.
- Open, save and save as, asking before unsaved changes are lost.
- Compilation with `pdfmom` on save and with `F5`. A new save cancels the run
  in progress, and a failed run keeps the previous PDF.
- PDF preview that renders only the pages near the viewport, fits to the width
  by default, zooms with `Ctrl` + wheel, and keeps the page and position across
  reloads.
- groff's messages in the log as they arrive, with severity, program, file and
  line.
- Preferences for compiling on save, the command, and the tbl, eqn, pic and
  refer preprocessors.
- A clear message when `pdfmom` is not installed.
