// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs `pdfmom` and parses the diagnostics groff writes to stderr.
//!
//! This module only depends on `glib`/`gio` (no GTK), so the parser can be
//! unit tested without a display.

use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gio::prelude::*;
use regex::Regex;

/// How serious a line of `pdfmom` output is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// One line of compiler output, parsed as far as possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Program that reported it (`troff`, `tbl`, `gropdf`, `mom`, …).
    pub program: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

impl Diagnostic {
    fn info(text: &str) -> Self {
        Self {
            severity: Severity::Info,
            program: None,
            file: None,
            line: None,
            message: text.to_owned(),
        }
    }
}

/// `troff:file.mom:12: warning: message` (the program prefix is optional,
/// and older groff versions put a space after it).
fn located_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:(?P<prog>[^\s:]+):\s*)?(?P<file>[^\s:][^:]*):(?P<line>\d+):(?:\d+:)?\s*(?:(?P<sev>fatal error|error|warning|note|debug)(?:\s*\[[^\]]*\])?:\s*)?(?P<msg>.*)$",
        )
        .expect("located diagnostic regex is valid")
    })
}

/// `/usr/bin/gropdf: warning: message` or `/usr/bin/gropdf:doc.mom: warning:
/// message` (no line number).
fn program_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<prog>[^\s:]+)(?::(?P<file>[^\s:][^:]*))?:\s*(?P<sev>fatal error|error|warning|note|debug)(?:\s*\[[^\]]*\])?:\s*(?P<msg>.*)$",
        )
        .expect("program diagnostic regex is valid")
    })
}

/// Messages from the mom macros: `[mom]: Font style "ZZ" at line 5 …`.
fn mom_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[mom\]:?\s*(?P<msg>.*)$").expect("mom regex is valid"))
}

fn mom_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bline (\d+)").expect("mom line regex is valid"))
}

fn mom_file_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|\bof |\bAborting )'([^']+)'").expect("mom file regex is valid")
    })
}

fn severity_from(keyword: Option<&str>) -> Severity {
    match keyword {
        Some("warning") => Severity::Warning,
        Some("note") | Some("debug") => Severity::Info,
        // `error`, `fatal error`, or a located message without a keyword.
        _ => Severity::Error,
    }
}

/// `/usr/bin/gropdf` → `gropdf`.
fn program_name(raw: &str) -> String {
    raw.rsplit('/').next().unwrap_or(raw).to_owned()
}

/// Parses one line of `pdfmom` stderr. Blank lines yield `None`; lines that
/// are not recognised come back as [`Severity::Info`] with the raw text.
pub fn parse_line(line: &str) -> Option<Diagnostic> {
    let line = line.trim_end();
    if line.trim().is_empty() {
        return None;
    }
    // Continuation lines (eqn context, wrapped mom messages) are indented.
    if line.starts_with(char::is_whitespace) {
        return Some(Diagnostic::info(line.trim()));
    }

    if let Some(caps) = mom_re().captures(line) {
        let message = caps["msg"].to_owned();
        let severity = if message.contains("deprecated") {
            Severity::Warning
        } else {
            Severity::Error
        };
        return Some(Diagnostic {
            severity,
            program: Some("mom".into()),
            file: mom_file_re().captures(&message).map(|c| c[1].to_owned()),
            line: mom_line_re()
                .captures(&message)
                .and_then(|c| c[1].parse().ok()),
            message,
        });
    }

    if let Some(caps) = located_re().captures(line) {
        return Some(Diagnostic {
            severity: severity_from(caps.name("sev").map(|m| m.as_str())),
            program: caps.name("prog").map(|m| program_name(m.as_str())),
            file: Some(caps["file"].to_owned()),
            line: caps["line"].parse().ok(),
            message: caps["msg"].to_owned(),
        });
    }

    if let Some(caps) = program_re().captures(line) {
        return Some(Diagnostic {
            severity: severity_from(caps.name("sev").map(|m| m.as_str())),
            program: Some(program_name(&caps["prog"])),
            file: caps.name("file").map(|m| m.as_str().to_owned()),
            line: None,
            message: caps["msg"].to_owned(),
        });
    }

    Some(Diagnostic::info(line))
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("el comando de compilación está vacío")]
    EmptyCommand,
    #[error("el comando de compilación no es válido: {0}")]
    InvalidCommand(glib::Error),
    #[error("la ruta del documento no es válida: {0}")]
    InvalidSource(String),
    #[error("no se pudo ejecutar el compilador: {0}")]
    Spawn(glib::Error),
    #[error("error al esperar al compilador: {0}")]
    Wait(glib::Error),
    #[error("no se pudo colocar el PDF: {0}")]
    Io(#[from] std::io::Error),
}

/// Compiler settings, independent from where they are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOptions {
    /// Command line, parsed with shell rules (e.g. `pdfmom` or `pdfmom -k`).
    pub command: String,
    pub tbl: bool,
    pub eqn: bool,
    pub pic: bool,
    pub refer: bool,
}

/// Builds the argument vector: the configured command, `-k` so the source
/// is read as UTF-8, the preprocessor flags and finally the file name.
pub fn build_argv(
    options: &CompileOptions,
    file_name: &OsStr,
) -> Result<Vec<OsString>, CompileError> {
    if options.command.trim().is_empty() {
        return Err(CompileError::EmptyCommand);
    }
    let mut argv =
        glib::shell_parse_argv(options.command.as_str()).map_err(CompileError::InvalidCommand)?;
    if argv.is_empty() {
        return Err(CompileError::EmptyCommand);
    }
    let mut push_flag = |flag: &str| {
        if !argv.iter().any(|a| a == flag) {
            argv.push(flag.into());
        }
    };
    push_flag("-k");
    for (enabled, flag) in [
        (options.tbl, "-t"),
        (options.eqn, "-e"),
        (options.pic, "-p"),
        (options.refer, "-R"),
    ] {
        if enabled {
            push_flag(flag);
        }
    }
    argv.push(file_name.to_owned());
    Ok(argv)
}

/// Returns the program of the command line, if it can be found in `PATH`
/// (or, for a path, if it exists).
pub fn find_program(command: &str) -> Option<PathBuf> {
    let argv = glib::shell_parse_argv(command).ok()?;
    let program = argv.first()?;
    glib::find_program_in_path(program)
}

/// Handle used to cancel a running compilation.
#[derive(Debug, Default)]
pub struct Job {
    cancelled: Cell<bool>,
    pgid: Cell<Option<i32>>,
}

impl Job {
    pub fn new() -> Self {
        Self::default()
    }

    /// Kills the whole process group (pdfmom, groff, troff, gropdf…).
    pub fn cancel(&self) {
        self.cancelled.set(true);
        if let Some(pgid) = self.pgid.get() {
            // SAFETY: plain syscall; a negative pid targets the process group
            // created for this compilation in the child setup below.
            unsafe {
                libc::kill(-pgid, libc::SIGTERM);
            }
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
}

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub source: PathBuf,
    pub options: CompileOptions,
}

#[derive(Debug, Clone)]
pub struct Report {
    /// The PDF was produced and moved into place.
    pub success: bool,
    pub pdf: PathBuf,
    pub errors: usize,
    pub warnings: usize,
    /// Exit status, or `None` if the process was killed by a signal.
    pub exit_status: Option<i32>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Finished(Report),
    Cancelled,
}

/// Where the PDF for `source` is written: same directory, `.pdf` extension.
pub fn pdf_path(source: &Path) -> PathBuf {
    source.with_extension("pdf")
}

fn temp_path(dir: &Path, stem: &OsStr) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = OsString::from(".");
    name.push(stem);
    name.push(format!(".myne-{}-{n}.pdf.part", std::process::id()));
    dir.join(name)
}

/// Compiles `request.source` with `pdfmom`, calling `on_diagnostic` for each
/// line of stderr as it arrives.
///
/// The PDF is written to a temporary file next to the source and renamed over
/// `<name>.pdf` only if the compilation succeeds, so a failed run keeps the
/// previous PDF. groff exits with status 0 even after errors, so any error
/// diagnostic also counts as a failure.
pub async fn run(
    request: &CompileRequest,
    job: &Job,
    mut on_diagnostic: impl FnMut(&Diagnostic),
) -> Result<Outcome, CompileError> {
    let start = Instant::now();
    let source = &request.source;
    let invalid = || CompileError::InvalidSource(source.display().to_string());
    let dir = source
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(invalid)?;
    let file_name = source.file_name().ok_or_else(invalid)?;
    let stem = source.file_stem().ok_or_else(invalid)?;
    let pdf = pdf_path(source);
    if pdf == *source {
        return Err(invalid());
    }

    let argv = build_argv(&request.options, file_name)?;
    let argv_ref: Vec<&OsStr> = argv.iter().map(OsString::as_os_str).collect();
    let tmp = temp_path(dir, stem);

    let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDERR_PIPE);
    launcher.set_cwd(dir);
    launcher.set_stdout_file_path(Some(&tmp));
    launcher.set_child_setup(|| {
        // SAFETY: setpgid is async-signal-safe; it puts pdfmom and all of its
        // children in a new process group so they can be killed together.
        unsafe {
            libc::setpgid(0, 0);
        }
    });

    let process = match launcher.spawn(&argv_ref) {
        Ok(p) => p,
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(CompileError::Spawn(err));
        }
    };
    job.pgid
        .set(process.identifier().and_then(|id| id.parse().ok()));
    if job.is_cancelled() {
        job.cancel();
    }

    let mut errors = 0;
    let mut warnings = 0;
    if let Some(stderr) = process.stderr_pipe() {
        let reader = gio::DataInputStream::new(&stderr);
        loop {
            match reader.read_line_utf8_future(glib::Priority::DEFAULT).await {
                Ok(Some(line)) => {
                    if let Some(diag) = parse_line(&line) {
                        match diag.severity {
                            Severity::Error => errors += 1,
                            Severity::Warning => warnings += 1,
                            Severity::Info => {}
                        }
                        on_diagnostic(&diag);
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    on_diagnostic(&Diagnostic::info(&format!(
                        "(salida ilegible del compilador: {err})"
                    )));
                    break;
                }
            }
        }
    }

    let waited = process.wait_future().await;
    if job.is_cancelled() {
        let _ = std::fs::remove_file(&tmp);
        return Ok(Outcome::Cancelled);
    }
    if let Err(err) = waited {
        let _ = std::fs::remove_file(&tmp);
        return Err(CompileError::Wait(err));
    }

    let exit_status = process.has_exited().then(|| process.exit_status());
    let produced = std::fs::metadata(&tmp)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    let success = exit_status == Some(0) && errors == 0 && produced;
    if success {
        if let Err(err) = std::fs::rename(&tmp, &pdf) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err.into());
        }
    } else {
        let _ = std::fs::remove_file(&tmp);
    }

    Ok(Outcome::Finished(Report {
        success,
        pdf,
        errors,
        warnings,
        exit_status,
        elapsed: start.elapsed(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(
        sev: Severity,
        prog: Option<&str>,
        file: Option<&str>,
        line: Option<u32>,
        msg: &str,
    ) -> Diagnostic {
        Diagnostic {
            severity: sev,
            program: prog.map(Into::into),
            file: file.map(Into::into),
            line,
            message: msg.into(),
        }
    }

    // All samples below are real output of groff 1.23.0 (`pdfmom`).

    #[test]
    fn troff_warning() {
        assert_eq!(
            parse_line("troff:bad.mom:6: warning: cannot select font 'ZZ'"),
            Some(d(
                Severity::Warning,
                Some("troff"),
                Some("bad.mom"),
                Some(6),
                "cannot select font 'ZZ'"
            ))
        );
    }

    #[test]
    fn troff_error() {
        assert_eq!(
            parse_line(
                "troff:bad.mom:9: error: can't open 'noexiste.mom': No such file or directory"
            ),
            Some(d(
                Severity::Error,
                Some("troff"),
                Some("bad.mom"),
                Some(9),
                "can't open 'noexiste.mom': No such file or directory"
            ))
        );
    }

    #[test]
    fn troff_warning_in_macro_package() {
        let diag = parse_line(
            "troff:/usr/share/groff/1.23.0/tmac/om.tmac:20289: warning: macro 'PDFBOOKMARK.NAME' not defined",
        )
        .expect("parsed");
        assert_eq!(
            diag.file.as_deref(),
            Some("/usr/share/groff/1.23.0/tmac/om.tmac")
        );
        assert_eq!(diag.line, Some(20289));
        assert_eq!(diag.severity, Severity::Warning);
    }

    #[test]
    fn tbl_and_eqn_errors() {
        assert_eq!(
            parse_line("tbl:s.mom:7: error: invalid column classifier 'f'"),
            Some(d(
                Severity::Error,
                Some("tbl"),
                Some("s.mom"),
                Some(7),
                "invalid column classifier 'f'"
            ))
        );
        assert_eq!(
            parse_line("eqn:s.mom:3: error: syntax error"),
            Some(d(
                Severity::Error,
                Some("eqn"),
                Some("s.mom"),
                Some(3),
                "syntax error"
            ))
        );
    }

    #[test]
    fn program_without_location() {
        assert_eq!(
            parse_line("/usr/bin/gropdf: warning: failed to open PDF 'noexiste.pdf'"),
            Some(d(
                Severity::Warning,
                Some("gropdf"),
                None,
                None,
                "failed to open PDF 'noexiste.pdf'"
            ))
        );
        assert_eq!(
            parse_line("tbl: fatal error: can't open 'noexiste.mom': No such file or directory"),
            Some(d(
                Severity::Error,
                Some("tbl"),
                None,
                None,
                "can't open 'noexiste.mom': No such file or directory"
            ))
        );
    }

    #[test]
    fn program_with_file_but_no_line() {
        // Seen with `pdfmom -k`.
        assert_eq!(
            parse_line("/usr/bin/gropdf:bad.mom: warning: failed to open PDF 'noexiste.pdf'"),
            Some(d(
                Severity::Warning,
                Some("gropdf"),
                Some("bad.mom"),
                None,
                "failed to open PDF 'noexiste.pdf'"
            ))
        );
        // `.so` of a missing file on the last line: troff gives no location.
        assert_eq!(
            parse_line("troff: error: can't open 'noexiste.mom': No such file or directory"),
            Some(d(
                Severity::Error,
                Some("troff"),
                None,
                None,
                "can't open 'noexiste.mom': No such file or directory"
            ))
        );
    }

    #[test]
    fn mom_messages() {
        assert_eq!(
            parse_line("[mom]: Font style \"ZZ\" at line 5 has not been registered."),
            Some(d(
                Severity::Error,
                Some("mom"),
                None,
                Some(5),
                "Font style \"ZZ\" at line 5 has not been registered."
            ))
        );
        assert_eq!(
            parse_line("[mom]: Aborting 'r.mom' at FONT, line 5."),
            Some(d(
                Severity::Error,
                Some("mom"),
                Some("r.mom"),
                Some(5),
                "Aborting 'r.mom' at FONT, line 5."
            ))
        );
        assert_eq!(
            parse_line("[mom]: PRINTSTYLE missing.  Aborting 'm.mom'."),
            Some(d(
                Severity::Error,
                Some("mom"),
                Some("m.mom"),
                None,
                "PRINTSTYLE missing.  Aborting 'm.mom'."
            ))
        );
        let diag = parse_line("[mom]: 's.mom' contains tbl(1) data but '-t' flag is missing from")
            .expect("parsed");
        assert_eq!(diag.file.as_deref(), Some("s.mom"));
    }

    #[test]
    fn continuation_and_unknown_lines() {
        assert_eq!(
            parse_line(" context is"),
            Some(d(Severity::Info, None, None, None, "context is"))
        );
        assert_eq!(
            parse_line("\tx over >>> end of input <<<"),
            Some(d(
                Severity::Info,
                None,
                None,
                None,
                "x over >>> end of input <<<"
            ))
        );
        assert_eq!(
            parse_line("Aborting."),
            Some(d(Severity::Info, None, None, None, "Aborting."))
        );
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("   "), None);
    }

    #[test]
    fn older_groff_format_with_space() {
        let diag =
            parse_line("troff: doc.mom:3: warning: numeric expression expected").expect("parsed");
        assert_eq!(diag.program.as_deref(), Some("troff"));
        assert_eq!(diag.file.as_deref(), Some("doc.mom"));
        assert_eq!(diag.line, Some(3));
    }

    fn options(command: &str) -> CompileOptions {
        CompileOptions {
            command: command.into(),
            tbl: false,
            eqn: false,
            pic: false,
            refer: false,
        }
    }

    #[test]
    fn argv_defaults_and_flags() {
        let argv = build_argv(&options("pdfmom"), OsStr::new("a b.mom")).expect("argv");
        assert_eq!(argv, ["pdfmom", "-k", "a b.mom"].map(OsString::from));

        let mut opts = options("pdfmom -k -wall");
        opts.tbl = true;
        opts.refer = true;
        let argv = build_argv(&opts, OsStr::new("x.mom")).expect("argv");
        assert_eq!(
            argv,
            ["pdfmom", "-k", "-wall", "-t", "-R", "x.mom"].map(OsString::from)
        );
    }

    #[test]
    fn argv_rejects_empty_command() {
        assert!(matches!(
            build_argv(&options("  "), OsStr::new("x.mom")),
            Err(CompileError::EmptyCommand)
        ));
    }

    #[test]
    fn pdf_next_to_source() {
        assert_eq!(
            pdf_path(Path::new("/tmp/doc.mom")),
            PathBuf::from("/tmp/doc.pdf")
        );
    }
}
