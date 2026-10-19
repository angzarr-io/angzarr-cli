//! angzarr-fmt for clap. A program flattens [`FormatArgs`] into its root
//! command, builds one [`Reporter`] from it, writes warnings through
//! [`Reporter::warn`], and ends every command through [`Reporter::finish`],
//! which writes the result to stdout or the failure to stderr and picks the
//! exit status.

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use angzarr_fmt::{
    Catalog, Diagnostic, Diagnostics, Failure, Format, RenderCtx, Report, Viewable, Warning,
    diagnostic_format,
};
use clap::builder::PossibleValue;
use clap::{Args, ValueEnum};

/// The help text for `--format`.
pub const FORMAT_HELP: &str = "Output format: json, yaml, toml, text or markdown [default: text on a terminal, json otherwise]";

/// A [`Format`] as a clap value, completed and validated against exactly
/// [`Format::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatChoice(pub Format);

const CHOICES: [FormatChoice; 5] = [
    FormatChoice(Format::Json),
    FormatChoice(Format::Yaml),
    FormatChoice(Format::Toml),
    FormatChoice(Format::Text),
    FormatChoice(Format::Markdown),
];

impl ValueEnum for FormatChoice {
    fn value_variants<'a>() -> &'a [Self] {
        &CHOICES
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.0.name()))
    }
}

/// One global `--format` for a command tree. Unset follows the terminal.
#[derive(Args, Clone, Debug, Default)]
pub struct FormatArgs {
    #[arg(long, global = true, value_enum, value_name = "FORMAT", help = FORMAT_HELP)]
    pub format: Option<FormatChoice>,
}

impl FormatArgs {
    /// Whether the user asked for a format.
    pub fn is_explicit(&self) -> bool {
        self.format.is_some()
    }

    /// The format this invocation writes in: the flag when given, text when
    /// stdout is a terminal, `off_terminal` otherwise.
    pub fn resolve(&self, off_terminal: Format) -> Format {
        angzarr_fmt::resolve(
            self.format.map(|c| c.0),
            io::stdout().is_terminal(),
            off_terminal,
        )
    }

    /// The reporter for this invocation.
    pub fn reporter<'a>(
        &self,
        off_terminal: Format,
        catalog: &'a Catalog,
        prog: &'a str,
    ) -> Reporter<'a> {
        Reporter::new(
            self.resolve(off_terminal),
            self.is_explicit(),
            catalog,
            prog,
        )
    }
}

/// A command's result and the exit status it ends with. Most results exit
/// 0; a lint that found errors writes its findings and still fails.
#[derive(Debug)]
pub struct Outcome<T> {
    pub value: T,
    pub status: u8,
}

impl<T> Outcome<T> {
    /// A result that exits 0.
    pub fn ok(value: T) -> Outcome<T> {
        Outcome { value, status: 0 }
    }

    /// A result written in full that exits with `status`.
    pub fn with_status(value: T, status: u8) -> Outcome<T> {
        Outcome { value, status }
    }
}

impl<T> From<T> for Outcome<T> {
    fn from(value: T) -> Outcome<T> {
        Outcome::ok(value)
    }
}

impl Outcome<Diagnostics> {
    /// Findings as a result: exit 1 when any is an error, 0 otherwise.
    pub fn findings(value: Diagnostics) -> Outcome<Diagnostics> {
        let status = u8::from(value.has_errors());
        Outcome { value, status }
    }
}

/// Where one invocation's output goes: the format, the catalog its
/// diagnostics render with, and the program name warnings carry.
///
/// A failure or warning goes to stderr in the structured format only when
/// the format was asked for or stderr is not a terminal; a person at a
/// terminal who did not ask reads text there ([`diagnostic_format`]).
pub struct Reporter<'a> {
    format: Format,
    explicit: bool,
    stderr_terminal: bool,
    catalog: &'a Catalog,
    prog: &'a str,
}

impl<'a> Reporter<'a> {
    /// A reporter writing results in `format`; `explicit` says whether the
    /// user asked for it.
    pub fn new(format: Format, explicit: bool, catalog: &'a Catalog, prog: &'a str) -> Self {
        Reporter {
            format,
            explicit,
            stderr_terminal: io::stderr().is_terminal(),
            catalog,
            prog,
        }
    }

    /// The format results are written in.
    pub fn format(&self) -> Format {
        self.format
    }

    /// The catalog diagnostics render with.
    pub fn catalog(&self) -> &'a Catalog {
        self.catalog
    }

    /// The context results render with.
    pub fn ctx(&self) -> RenderCtx<'a> {
        RenderCtx {
            catalog: self.catalog,
        }
    }

    fn stderr_format(&self) -> Format {
        diagnostic_format(self.format, self.explicit, self.stderr_terminal)
    }

    /// Writes one warning to stderr. A warning that cannot be written is
    /// dropped: it must not end the command.
    pub fn warn(&self, diagnostic: &dyn Diagnostic, remedy: Option<Report>) {
        let _ = self.warn_to(&mut io::stderr().lock(), diagnostic, remedy);
    }

    fn warn_to(
        &self,
        err: &mut dyn Write,
        diagnostic: &dyn Diagnostic,
        remedy: Option<Report>,
    ) -> Result<(), angzarr_fmt::FmtError> {
        let warning = Warning {
            prog: self.prog,
            diagnostic,
            remedy,
        };
        angzarr_fmt::write_warning(err, self.stderr_format(), self.catalog, &warning)
    }

    /// Ends a command: the result to stdout and the outcome's status, or
    /// the failure to stderr and the status the failure names. `E` is any
    /// [`Failure`], or `Box<dyn Failure>` for a command with several.
    pub fn finish<T: Viewable, E: Into<Box<dyn Failure>>>(
        &self,
        result: Result<Outcome<T>, E>,
    ) -> ExitCode {
        ExitCode::from(self.finish_to(&mut io::stdout().lock(), &mut io::stderr().lock(), result))
    }

    fn finish_to<T: Viewable, E: Into<Box<dyn Failure>>>(
        &self,
        out: &mut dyn Write,
        err: &mut dyn Write,
        result: Result<Outcome<T>, E>,
    ) -> u8 {
        let failure: Box<dyn Failure> = match result {
            Ok(outcome) => {
                let written = angzarr_fmt::render(out, self.format, &self.ctx(), &outcome.value)
                    .and_then(|()| out.flush().map_err(angzarr_fmt::FmtError::from));
                match written {
                    Ok(()) => return outcome.status,
                    Err(e) => Box::new(e),
                }
            }
            Err(f) => f.into(),
        };
        // A failure that cannot be written in its format still reaches the
        // user, as text.
        let format = self.stderr_format();
        if angzarr_fmt::write_failure(err, format, self.catalog, &*failure).is_err() {
            let _ = angzarr_fmt::write_failure(err, Format::Text, self.catalog, &*failure);
        }
        failure.exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[derive(Parser)]
    struct Prog {
        #[command(flatten)]
        format: FormatArgs,
        #[command(subcommand)]
        command: Option<Sub>,
    }

    #[derive(clap::Subcommand)]
    enum Sub {
        Child,
    }

    #[test]
    fn format_is_one_global_flag_offering_exactly_the_supported_formats() {
        let cmd = Prog::command();
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id() == "format")
            .unwrap();
        assert!(arg.is_global_set());
        let offered: Vec<String> = arg
            .get_possible_values()
            .iter()
            .map(|v| v.get_name().to_string())
            .collect();
        assert_eq!(offered, ["json", "yaml", "toml", "text", "markdown"]);
    }

    #[test]
    fn the_flag_is_accepted_after_a_subcommand() {
        let prog = Prog::try_parse_from(["prog", "child", "--format", "yaml"]).unwrap();
        assert_eq!(prog.format.format, Some(FormatChoice(Format::Yaml)));
    }

    #[test]
    fn an_unknown_format_is_refused_by_the_parser() {
        assert!(Prog::try_parse_from(["prog", "--format", "xml"]).is_err());
    }

    #[test]
    fn an_explicit_flag_resolves_regardless_of_the_terminal() {
        let args = FormatArgs {
            format: Some(FormatChoice(Format::Toml)),
        };
        assert_eq!(args.resolve(Format::Json), Format::Toml);
        assert!(args.is_explicit());
        assert!(!FormatArgs::default().is_explicit());
    }

    use angzarr_fmt::{FmtError, Severity};

    fn catalog() -> Catalog {
        let mut c = Catalog::en_us();
        c.add(angzarr_fmt::EN_US).unwrap();
        c
    }

    struct Ended {
        status: u8,
        out: String,
        err: String,
    }

    fn end<T: Viewable, E: Into<Box<dyn Failure>>>(
        format: Format,
        explicit: bool,
        stderr_terminal: bool,
        result: Result<Outcome<T>, E>,
    ) -> Ended {
        let catalog = catalog();
        let reporter = Reporter {
            format,
            explicit,
            stderr_terminal,
            catalog: &catalog,
            prog: "prog",
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = reporter.finish_to(&mut out, &mut err, result);
        Ended {
            status,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        }
    }

    #[test]
    fn a_result_goes_to_stdout_with_its_status() {
        let ended = end(
            Format::Json,
            false,
            false,
            Ok::<_, FmtError>(Outcome::with_status(Diagnostics::new(), 3)),
        );
        assert_eq!(ended.status, 3);
        assert_eq!(ended.out, "[]\n");
        assert!(ended.err.is_empty());
    }

    #[test]
    fn findings_with_an_error_are_written_and_exit_1() {
        let mut d = Diagnostics::new();
        d.push(Severity::Error, None, Box::new(Report::new("t-x", "bad")));
        let ended = end(
            Format::Json,
            false,
            false,
            Ok::<_, FmtError>(Outcome::findings(d)),
        );
        assert_eq!(ended.status, 1);
        assert!(ended.out.contains("\"severity\": \"error\""));
        let ended = end(
            Format::Json,
            false,
            false,
            Ok::<_, FmtError>(Outcome::findings(Diagnostics::new())),
        );
        assert_eq!(ended.status, 0);
    }

    #[test]
    fn a_failure_goes_to_stderr_as_the_envelope_off_a_terminal() {
        let ended = end(
            Format::Json,
            false,
            false,
            Err::<Outcome<Diagnostics>, _>(FmtError::TomlNull { path: "a".into() }),
        );
        assert_eq!(ended.status, 1);
        assert!(ended.out.is_empty());
        assert!(
            ended
                .err
                .starts_with("{\n  \"error\": {\n    \"code\": \"angzarr-fmt-toml-null\""),
            "{}",
            ended.err
        );
        assert!(
            ended
                .err
                .contains("\"code\": \"angzarr-fmt-use-json-or-yaml\""),
            "{}",
            ended.err
        );
    }

    #[test]
    fn a_failure_is_text_at_a_terminal_unless_the_format_was_asked_for() {
        let failure = || Err::<Outcome<Diagnostics>, _>(FmtError::TomlNull { path: "a".into() });
        let ended = end(Format::Json, false, true, failure());
        assert!(
            ended.err.starts_with("Error: TOML has no null"),
            "{}",
            ended.err
        );
        let ended = end(Format::Json, true, true, failure());
        assert!(ended.err.starts_with('{'), "{}", ended.err);
    }

    #[test]
    fn a_result_that_cannot_be_written_is_a_failure() {
        let ended = end(
            Format::Toml,
            true,
            false,
            Ok::<_, FmtError>(Outcome::ok(Diagnostics::new())),
        );
        assert_eq!(ended.status, 1);
        assert!(ended.out.is_empty());
        assert!(
            ended.err.contains("angzarr-fmt-toml-needs-table"),
            "{}",
            ended.err
        );
    }

    #[test]
    fn any_failure_boxes_so_a_command_can_question_mark_several() {
        fn command(fail: bool) -> Result<Outcome<Diagnostics>, Box<dyn Failure>> {
            if fail {
                Err(FmtError::TomlNull { path: "a".into() })?;
            }
            Ok(Diagnostics::new().into())
        }
        assert_eq!(end(Format::Json, false, false, command(true)).status, 1);
        assert_eq!(end(Format::Json, false, false, command(false)).status, 0);
    }

    #[test]
    fn a_warning_is_json_lines_off_a_terminal_and_text_at_one() {
        let catalog = catalog();
        let d = FmtError::TomlNull { path: "a".into() };
        for (stderr_terminal, starts) in [(false, "{"), (true, "prog: warning: ")] {
            let reporter = Reporter {
                format: Format::Json,
                explicit: false,
                stderr_terminal,
                catalog: &catalog,
                prog: "prog",
            };
            let mut err = Vec::new();
            reporter.warn_to(&mut err, &d, d.remedy()).unwrap();
            let err = String::from_utf8(err).unwrap();
            assert!(err.starts_with(starts), "{err}");
        }
    }
}
