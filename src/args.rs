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
pub fn parse() -> Result<Options, ArgsError> {
    parse_from_argv(env::args_os())
}

/// Parse application options from given array of arguments
/// (*all* arguments, including binary name).
#[inline]
pub fn parse_from_argv<I, T>(argv: I) -> Result<Options, ArgsError>
    where I: IntoIterator<Item=T>, T: Clone + Into<OsString>
{
    let argv: Vec<_> = argv.into_iter().collect();

    // We allow `gisht JohnDoe/foo` to be an alias of `gisht run JohnDoe/foo`.
    // To support this, some preprocessing on the arguments has to be done
    // in order to pick the parser with or without subcommands.
    let parser = {
        // Determine whether the first non-flag argument is one of the gist commands.
        let first_arg = argv.iter().skip(1)
            .map(|arg| {
                let arg: OsString = arg.clone().into();
                arg.into_string().unwrap_or_else(|_| String::new())
            })
            .find(|arg| !arg.starts_with("-"))
            .unwrap_or_else(|| String::new());

        // If it is, use the full argument parser which recognizes those commands.
        match Command::from_str(&first_arg) {
            Ok(_) => create_full_parser(),
            Err(_) => {
                // If it's not, the parser will already have "run" command baked in.
                let mut parser = create_parser_base();
                parser = configure_run_gist_parser(parser);
                parser
            },
        }
    };

    let matches = parser.get_matches_from(argv);
    Options::try_from(matches)
}


/// Structure to hold options received from the command line.
#[derive(Clone, Debug)]
pub struct Options {
    /// Verbosity of the logging output.
    ///
    /// Corresponds to the number of times the -v flag has been passed.
    /// If -q has been used instead, this will be negative.
    pub verbosity: isize,
    /// Gist locality flag.
    ///
    /// Depending on its value, this flag. may optionally
    /// e.g. prohibit the app from downloading gists from remote hosts.
    pub locality: Option<Locality>,
    /// Gist command that's been issued.
    pub command: Command,
    /// URI to the gist to operate on.
    pub gist_uri: gist::Uri,
    /// Arguments to the gist, if any.
    /// This is only used if command == Some(Command::Run).
    pub gist_args: Option<Vec<String>>,
}

impl Options {
    #[inline]
    pub fn verbose(&self) -> bool { self.verbosity > 0 }
    #[inline]
    pub fn quiet(&self) -> bool { self.verbosity < 0 }
}

impl<'a> TryFrom<ArgMatches<'a>> for Options {
    type Err = ArgsError;

    fn try_from(matches: ArgMatches<'a>) -> Result<Self, Self::Err> {
        let verbose_count = matches.occurrences_of(OPT_VERBOSE) as isize;
        let quiet_count = matches.occurrences_of(OPT_QUIET) as isize;
        let verbosity = verbose_count - quiet_count;

        let locality = if matches.is_present(OPT_LOCAL) {
            Some(Locality::Local)
        } else if matches.is_present(OPT_REMOTE) {
            Some(Locality::Remote)
        } else {
            None
        };

        // Command may be optionally provided.
        // If it isn't, it means the "run"  default was used, and so all the arguments
        // are arguments to `gisht run`.
        let (cmd, cmd_matches) = matches.subcommand();
        let cmd_matches = cmd_matches.unwrap_or(&matches);
        let command = Command::from_str(cmd).unwrap_or(Command::Run);

        // Parse out the gist URI argument.
        let gist_uri = try!(gist::Uri::from_str(
            cmd_matches.value_of(ARG_GIST).unwrap()
        ));

        // For the "run" command, arguments may be provided.
        let mut gist_args = cmd_matches.values_of(ARG_GIST_ARGV)
            .map(|argv| argv.map(|v| v.to_owned()).collect());
        if command == Command::Run && gist_args.is_none() {
            gist_args = Some(vec![]);
        }

        Ok(Options{
            verbosity: verbosity,
            locality: locality,
            command: command,
            gist_uri: gist_uri,
            gist_args: gist_args,
        })
    }
}

custom_derive! {
    /// Error that can occur while parsing of command line arguments.
    #[derive(Debug, Clone, PartialEq,
             Error("command line arguments error"), ErrorDisplay, ErrorFrom)]
    pub enum ArgsError {
        /// Error while parsing the gist URI.
        Gist(gist::UriError),
    }
}


custom_derive! {
    /// Enum describing gist "locality" options.
    #[derive(Clone, Debug, Eq, PartialEq,
             IterVariants(Localities))]
    pub enum Locality {
        /// Operate only on gists available locally
        /// (do not fetch anything from remote gist hosts).
        Local,
        /// Always fetch the gists from a remote gist host.
        Remote,
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
            "print" => Ok(Command::Print),
            "open" => Ok(Command::Open),
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
const ARG_GIST_ARGV: &'static str = "argv";
const OPT_VERBOSE: &'static str = "verbose";
const OPT_QUIET: &'static str = "quiet";
const OPT_LOCAL: &'static str = "local";
const OPT_REMOTE: &'static str = "remote";


/// Create the full argument parser.
/// This parser accepts the entire gamut of the application's arguments and flags.
fn create_full_parser<'p>() -> Parser<'p> {
    let parser = create_parser_base();

    parser
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::VersionlessSubcommands)

        .subcommand(configure_run_gist_parser(
            SubCommand::with_name(Command::Run.name())
                .about("Run the specified gist")))
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

/// Create the "base" argument parser object.
///
/// This base contains all the shared configuration (like the application name)
/// and the flags shared by all gist subcommands.
fn create_parser_base<'p>() -> Parser<'p> {
    let mut parser = Parser::new(APP_NAME);
    if let Some(version) = option_env!("CARGO_PKG_VERSION") {
        parser = parser.version(version);
    }
    parser
        .about(APP_DESC)

        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DeriveDisplayOrder)

        // Verbosity flags (shared by all subcommands).
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

        // Gist locality flags (shared by all subcommands).
        .arg(Arg::with_name(OPT_LOCAL)
            .long("cached").short("c")
            .conflicts_with(OPT_REMOTE)
            .help("Operate only on gists available locally"))
        .arg(Arg::with_name(OPT_REMOTE)
            .long("fetch").short("f")
            .conflicts_with(OPT_LOCAL)
            .help("Always fetch the gist from a remote host"))

        .help_short("H")
        .version_short("V")
}

/// Configure a parser for the "run" command.
/// This is also used when there is no command given.
fn configure_run_gist_parser<'p>(parser: Parser<'p>) -> Parser<'p> {
    parser
        .arg(gist_arg("Gist to run"))
        // This argument spec is capturing everything after the gist URI,
        // allowing for the arguments to be passed to the gist itself.
        .arg(Arg::with_name(ARG_GIST_ARGV)
            .required(false)
            .multiple(true)
            .use_delimiter(false)
            .help("Optional arguments passed to the gist")
            .value_name("ARGS"))
        .setting(AppSettings::TrailingVarArg)
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
