//! Module for handling command line arguments.

use std::env;
use std::ffi::OsString;
use std::iter::IntoIterator;
use std::str::FromStr;

use clap::{self, AppSettings, Arg, ArgMatches, ArgSettings, SubCommand};
use conv::TryFrom;
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
        let command = Command::try_from(matches.subcommand()).ok();
        let gist = command.as_ref()
            .and_then(|subcmd| matches.subcommand_matches(subcmd.name()))
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


/// Gist command issued to the application, along with its arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Run the specified gist.
    Run,
    /// Output the path to gist's binary.
    Which,
}

impl Command {
    fn name(&self) -> &'static str {
        match *self {
            Command::Run => "run",
            Command::Which => "which",
        }
    }
}

impl Default for Command {
    fn default() -> Self { Command::Run }
}

// Create a Command from the result of clap::ArgMatches::subcommand().
impl<'p, 's, 'a> TryFrom<(&'s str, Option<&'p ArgMatches<'a>>)> for Command {
    type Err = Unrepresentable<String>;

    fn try_from(input: (&'s str, Option<&'p ArgMatches<'a>>)) -> Result<Self, Self::Err> {
        match input {
            ("run", Some(_)) => Ok(Command::Run),
            ("which", Some(_)) => Ok(Command::Which),
            (cmd, _) => Err(Unrepresentable(cmd.to_owned())),
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
            .arg(Arg::with_name(ARG_GIST)
                .required(true)
                .help("Gist to run")
                .value_name("GIST")))
        .subcommand(SubCommand::with_name(Command::Which.name())
            .about("Output the path to gist's binary")
            .arg(Arg::with_name(ARG_GIST)
                .required(true)
                .help("Gist to locate")
                .value_name("GIST")))
}
