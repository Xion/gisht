//! Module for handling command line arguments.

use std::env;
use std::ffi::OsString;
use std::iter::IntoIterator;
use std::str::FromStr;

use clap::{self, AppSettings, Arg, ArgMatches, ArgSettings, SubCommand};
use conv::errors::Unrepresentable;

use super::gist;


/// Parse command line arguments and return matches' object.
#[inline]
pub fn parse() -> Options {
    parse_from_argv(env::args_os())
}

/// Parse application options from given array of arguments
/// (*all* arguments, including binary name).
#[inline]
pub fn parse_from_argv<I, T>(argv: I) -> Options
    where I: IntoIterator<Item=T>, T: Into<OsString>
{
    let matches = create_parser().get_matches_from(argv);
    Options::from(matches)
}


/// Structure to hold options received from the command line.
#[derive(Clone, Debug)]
pub struct Options {
    /// Verbosity of the logging output.
    ///
    /// Corresponds to the number of times the -v flag has been passed.
    /// If -q has been used instead, this will be negative.
    pub verbosity: isize,
    /// Gist command that's been issued, if any.
    pub command: Option<Command>,
    /// URI to the gist to operate on, if any.
    pub gist: Option<gist::Uri>,
}

impl Options {
    #[inline]
    pub fn verbose(&self) -> bool { self.verbosity > 0 }
    #[inline]
    pub fn quiet(&self) -> bool { self.verbosity < 0 }
}

impl<'a> From<ArgMatches<'a>> for Options {
    fn from(matches: ArgMatches<'a>) -> Self {
        let verbose_count = matches.occurrences_of(OPT_VERBOSE) as isize;
        let quiet_count = matches.occurrences_of(OPT_QUIET) as isize;

        // Command may be optionally provided, alongside the gist argument.
        let (subcmd, submatches) = matches.subcommand();
        let command = Command::from_str(subcmd).ok();
        let gist = submatches
            .and_then(|m| m.value_of(ARG_GIST))
            // TODO: fix error handling here (it should be TryFrom,
            // and failed gist URI parsing should return Err)
            .map(|g| gist::Uri::from_str(g).unwrap());

        Options{
            verbosity: verbose_count - quiet_count,
            command: command,
            gist: gist,
        }
    }
}


custom_derive! {
    /// Gist command issued to the application, along with its arguments.
    #[derive(Clone, Debug, Eq, PartialEq,
             IterVariants(Commands))]
    pub enum Command {
        /// Run the specified gist.
        Run,
        /// Output the path to gist's binary.
        Which,
        /// Print the complete source code of the gist's binary.
        Print,
        /// Open the gist's HTML page in the default web browser.
        Open,
    }
}

impl Command {
    fn name(&self) -> &'static str {
        match *self {
            Command::Run => "run",
            Command::Which => "which",
            Command::Print => "print",
            Command::Open => "open",
        }
    }
}

impl Default for Command {
    fn default() -> Self { Command::Run }
}

impl FromStr for Command {
    type Err = Unrepresentable<String>;

    /// Create a Command from the result of clap::ArgMatches::subcommand_name().
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(Command::Run),
            "which" => Ok(Command::Which),
            _ => Err(Unrepresentable(s.to_owned())),
        }
    }
}


// Parser configuration

/// Type of the argument parser object
/// (which is called an "App" in clap's silly nomenclature).
type Parser<'p> = clap::App<'p, 'p>;


const APP_NAME: &'static str = "gisht";
const APP_DESC: &'static str = "Gists in the shell";

const ARG_GIST: &'static str = "gist";
const OPT_VERBOSE: &'static str = "verbose";
const OPT_QUIET: &'static str = "quiet";


/// Create the argument parser.
fn create_parser<'p>() -> Parser<'p> {
    let mut parser = Parser::new(APP_NAME);
    if let Some(version) = option_env!("CARGO_PKG_VERSION") {
        parser = parser.version(version);
    }
    parser
        .about(APP_DESC)

        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DeriveDisplayOrder)

        // Flags shared by all subcommands.
        .arg(Arg::with_name(OPT_VERBOSE)
            .long("verbose").short("v")
            .set(ArgSettings::Multiple)
            .conflicts_with(OPT_QUIET)
            .help("Increase logging verbosity"))
        .arg(Arg::with_name(OPT_QUIET)
            .long("quiet").short("q")
            .set(ArgSettings::Multiple)
            .conflicts_with(OPT_VERBOSE)
            .help("Decrease logging verbosity"))
        .help_short("H")
        .version_short("V")

        .subcommand(SubCommand::with_name(Command::Run.name())
            .about("Run the specified gist")
            .arg(gist_arg("Gist to run")))
        .subcommand(SubCommand::with_name(Command::Which.name())
            .about("Output the path to gist's binary")
            .arg(gist_arg("Gist to locate")))
        .subcommand(SubCommand::with_name(Command::Print.name())
            .about("Print the source code of gist's binary")
            .arg(gist_arg("Gist to print")))
        .subcommand(SubCommand::with_name(Command::Open.name())
            .about("Open the gist's webpage")
            .arg(gist_arg("Gist to open")))
}

/// Create the GIST argument to various gist subcommands.
fn gist_arg(help: &'static str) -> Arg {
    Arg::with_name(ARG_GIST)
        .required(true)
        .help(help)
        .value_name("GIST")
}


#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use super::Command;

    #[test]
    fn command_names() {
        for cmd in Command::iter_variants() {
            assert_eq!(cmd, Command::from_str(cmd.name()).unwrap());
        }
    }
}
