//! angzarr-fmt for clap, the one place the two meet. The root command
//! flattens [`FormatArgs`], resolves the format once, and ends every command
//! through [`finish`], which writes the result to stdout or the failure to
//! stderr and picks the exit status.

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use angzarr_fmt::{Catalog, Failure, Format, Viewable};
use clap::builder::PossibleValue;
use clap::{Args, ValueEnum};

/// The help text for `--format`.
const FORMAT_HELP: &str =
    "Output format: json or text [default: text on a terminal, json otherwise]";

/// A [`Format`] as a clap value, completed and validated against exactly
/// [`Format::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatChoice(pub Format);

const CHOICES: [FormatChoice; 2] = [FormatChoice(Format::Json), FormatChoice(Format::Text)];

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
    /// The format this invocation writes in: the flag when given, text when
    /// stdout is a terminal, `off_terminal` otherwise.
    pub fn resolve(&self, off_terminal: Format) -> Format {
        angzarr_fmt::resolve(
            self.format.map(|c| c.0),
            io::stdout().is_terminal(),
            off_terminal,
        )
    }
}

/// Ends a command: the result to stdout in `format`, or the failure to
/// stderr in `format`, and the exit status the failure names.
pub fn finish<T: Viewable>(
    format: Format,
    catalog: &Catalog,
    result: Result<T, Box<dyn Failure>>,
) -> ExitCode {
    let failure = match result {
        Ok(value) => match angzarr_fmt::render(&mut io::stdout().lock(), format, &value) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(e) => Box::new(e) as Box<dyn Failure>,
        },
        Err(f) => f,
    };
    // A failure that cannot be written in the selected format still reaches
    // the user, as text.
    let mut stderr = io::stderr().lock();
    if angzarr_fmt::write_failure(&mut stderr, format, catalog, &*failure).is_err() {
        let _ = angzarr_fmt::write_failure(&mut stderr, Format::Text, catalog, &*failure);
    }
    ExitCode::from(failure.exit_code())
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
        assert_eq!(offered, ["json", "text"]);
    }

    #[test]
    fn the_flag_is_accepted_after_a_subcommand() {
        let prog = Prog::try_parse_from(["prog", "child", "--format", "text"]).unwrap();
        assert_eq!(prog.format.format, Some(FormatChoice(Format::Text)));
    }

    #[test]
    fn an_unknown_format_is_refused_by_the_parser() {
        assert!(Prog::try_parse_from(["prog", "--format", "xml"]).is_err());
    }

    #[test]
    fn an_explicit_flag_resolves_regardless_of_the_terminal() {
        let args = FormatArgs {
            format: Some(FormatChoice(Format::Text)),
        };
        assert_eq!(args.resolve(Format::Json), Format::Text);
    }
}
