//! Module for handling command line arguments.

use std::env;
use std::ffi::OsString;
use std::io;
use std::iter::IntoIterator;
use std::process::exit;
use std::str::FromStr;

use clap::{self, AppSettings, Arg, ArgMatches, ArgSettings, Shell, SubCommand};
use conv::TryFrom;
use conv::errors::Unrepresentable;
use url;

use super::{gist, NAME, VERSION};


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
        let maybe_first_arg = argv.iter().skip(1)
            .map(|arg| {
                let arg: OsString = arg.clone().into();
                arg.into_string().unwrap_or_else(|_| String::new())
            })
            .find(|arg| !arg.starts_with("-"));

        // (That is, provided we got a positional argument at all).
        let first_arg = maybe_first_arg.unwrap_or_else(|| "help".into());
        if first_arg != "help" {
            match Command::from_str(&first_arg) {
                Ok(_) => create_full_parser(),
                Err(_) => {
                    // If it's not a gist command, the parser we'll use
                    // will already have "run" command baked in.
                    let mut parser = create_parser_base();
                    parser = configure_run_gist_parser(parser);
                    parser
                },
            }
        } else {
            // If help was requested, use the full parser (with subcommands).
            // This ensure the correct help/usage instructions are shown.
            create_full_parser()
        }
    };

    let matches = try!(get_matches_with_completion(parser, argv));
    Options::try_from(matches)
}

/// Parse argv against given clap parser whilst handling the possible request
/// for generating autocompletion script for that parser.
fn get_matches_with_completion<'a, 'p, I, T>(parser: Parser<'p>, argv: I) -> Result<ArgMatches<'a>, clap::Error>
    where 'p: 'a, I: IntoIterator<Item=T>, T: Into<OsString> + Clone
{
    const OPT_COMPLETION: &'static str = "completion";

    let parser = parser
        // Hidden flag that's used to generate shell completion scripts.
        // It overrides the mandatory GIST arg.
        .arg(Arg::with_name(OPT_COMPLETION)
            .long("complete")
            .required(false).conflicts_with(ARG_GIST)
            .takes_value(true).number_of_values(1).multiple(false)
            .possible_values(&Shell::variants())
            .value_name("SHELL")
            .help("Generate autocompletion script for given shell")
            .set(ArgSettings::Hidden));

    let matches = try!(parser.get_matches_from_safe(argv));

    // If the completion flag was present, generate the scripts to stdout
    // and quit immediately.
    if let Some(shell) = matches.value_of(OPT_COMPLETION) {
        let shell = shell.parse::<Shell>().unwrap();
        debug!("Printing autocompletion script for {}...", shell);
        create_full_parser().gen_completions_to(*NAME, shell, &mut io::stdout());
        exit(0);
    }

    Ok(matches)
}


/// Structure to hold options received from the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Gist to operate on.
    pub gist: GistArg,
    /// Arguments to the gist, if any.
    /// This is only used if command == Some(Command::Run).
    pub gist_args: Option<Vec<String>>,
}

#[allow(dead_code)]
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

        // Parse out the gist argument.
        let gist = try!(GistArg::from_str(
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
            gist: gist,
            gist_args: gist_args,
        })
    }
}

custom_derive! {
    /// Error that can occur while parsing of command line arguments.
    #[derive(Debug,
             Error("command line arguments error"), ErrorDisplay, ErrorFrom)]
    pub enum ArgsError {
        /// General when parsing the arguments.
        Parse(clap::Error),
        /// Error while parsing the gist URI.
        Gist(GistError),
    }
}
impl PartialEq for ArgsError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // We have to write this branch (and the whole PartialEq implementation)
            // because clap::Error doesn't have its own PartialEq.
            // TODO: make a PR to clap to fix this.
            (&ArgsError::Parse(ref e1), &ArgsError::Parse(ref e2)) => {
                e1.message == e2.message && e1.kind == e2.kind && e1.info == e2.info
            },
            (&ArgsError::Gist(ref g1), &ArgsError::Gist(ref g2)) => g1 == g2,
            _ => false,
        }
    }
}


/// Type holding the value of the GIST argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GistArg {
    /// A gist URI, like "Octocat/hello" or "gh:Foo/bar"
    Uri(gist::Uri),
    /// A URL to a gist's browser page (that we hopefully recognize).
    BrowserUrl(url::Url),
}

impl FromStr for GistArg {
    type Err = GistError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // This is kind of a crappy heuristic but it should suffice for now.
        let s = input.trim().to_lowercase();
        let is_browser_url = ["http://", "https://", "www."].iter()
            .any(|p| s.starts_with(p));

        if is_browser_url {
            let gist_url = try!(url::Url::from_str(input));
            Ok(GistArg::BrowserUrl(gist_url))
        } else {
            let uri = try!(gist::Uri::from_str(input));
            Ok(GistArg::Uri(uri))
        }
    }
}

custom_derive! {
    /// Erorr that can occur while parsing of the GIST argument.
    #[derive(Debug, PartialEq,
             Error("gist argument error"), ErrorDisplay, ErrorFrom)]
    pub enum GistError {
        /// Error while parsing gist URI.
        Uri(gist::UriError),
        /// Error while parsing gist's browser URL.
        BrowserUrl(url::ParseError),
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
        /// Display summary information about the gist.
        Info,
    }
}

impl Command {
    /// Canonical name of this command.
    /// This is the name that the command will be shown under in the usage/help text.
    fn name(&self) -> &'static str {
        match *self {
            Command::Run => "run",
            Command::Which => "which",
            Command::Print => "print",
            Command::Open => "open",
            Command::Info => "info",
        }
    }

    /// Aliases (alternative names) for this command.
    /// These aliases are visible in the application's help message.
    fn aliases(&self) -> &'static [&'static str] {
        // Each possible result needs to have it's own named constant
        // because otherwise Rust cannot make them properly 'static -_-
        const RUN_ALIASES: &'static [&'static str] = &["exec"];
        const PRINT_ALIASES: &'static [&'static str] = &["cat"];
        const OPEN_ALIASES: &'static [&'static str] = &["show"];
        const INFO_ALIASES: &'static [&'static str] = &["stat"];

        match *self {
            Command::Run => RUN_ALIASES,
            Command::Print => PRINT_ALIASES,
            Command::Open => OPEN_ALIASES,
            Command::Info => INFO_ALIASES,
            _ => &[],
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
        for command in Command::iter_variants() {
            if command.name() == s || command.aliases().contains(&s) {
                return Ok(command);
            }
        }
        Err(Unrepresentable(s.to_owned()))
    }
}


// Parser configuration

/// Type of the argument parser object
/// (which is called an "App" in clap's silly nomenclature).
type Parser<'p> = clap::App<'p, 'p>;


lazy_static! {
    static ref ABOUT: &'static str = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
}

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
            subcommand_for(Command::Run)
                .about("Run the specified gist")))
        .subcommand(subcommand_for(Command::Which)
            .about("Output the path to gist's binary")
            .arg(gist_arg("Gist to locate")))
        .subcommand(subcommand_for(Command::Print)
            .about("Print the source code of gist's binary")
            .arg(gist_arg("Gist to print")))
        .subcommand(subcommand_for(Command::Open)
            .about("Open the gist's webpage")
            .arg(gist_arg("Gist to open")))
        .subcommand(subcommand_for(Command::Info)
            .about("Display summary information about the gist")
            .arg(gist_arg("Gist to display info on")))

        .after_help(
            "Hint: `gisht run GIST` can be shortened to just `gisht GIST`.\n\
            If you want to pass arguments, put them after `--` (two dashes), like this:\n\n\
            \tgisht Octocat/greet -- \"Hello world\" --cheerful")
}

/// Create the "base" argument parser object.
///
/// This base contains all the shared configuration (like the application name)
/// and the flags shared by all gist subcommands.
fn create_parser_base<'p>() -> Parser<'p> {
    let mut parser = Parser::new(*NAME);
    if let Some(version) = *VERSION {
        parser = parser.version(version);
    }
    parser
        .about(*ABOUT)

        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::ColorNever)

        // Gist locality flags (shared by all subcommands).
        .arg(Arg::with_name(OPT_LOCAL)
            .long("cached").short("c")
            .conflicts_with(OPT_REMOTE)
            .help("Operate only on gists available locally"))
        .arg(Arg::with_name(OPT_REMOTE)
            .long("fetch").short("f")
            .conflicts_with(OPT_LOCAL)
            .help("Always fetch the gist from a remote host"))

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

        .help_short("H")
        .version_short("V")
}

/// Create a clap subcommand Parser object for given gist Command.
fn subcommand_for<'p>(command: Command) -> Parser<'p> {
    SubCommand::with_name(command.name())
        .visible_aliases(command.aliases())
        .help_short("H")
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
    use super::{Command, create_full_parser, parse_from_argv};

    #[test]
    fn command_names() {
        for cmd in Command::iter_variants() {
            assert_eq!(cmd, Command::from_str(cmd.name()).unwrap());
        }
    }

    /// Check if all gist subcommands are actually used in the argparser.
    #[test]
    fn commands_in_usage() {
        // Usage will be returned as a failed parse Error's message.
        // Empty argument list ensures we actually get the parse error.
        let args: Vec<&'static str> = vec![];
        let usage = format!("{}", create_full_parser()
            .get_matches_from_safe(args).unwrap_err());

        for cmd in Command::iter_variants() {
            assert!(usage.contains(cmd.name()),
                "Usage string doesn't contain the '{}' command.", cmd.name());
        }
    }

    /// Verify that `run` subcommand is optional when running a gist without args.
    #[test]
    fn run_optional_no_args() {
        let run_opts = parse_from_argv(vec!["gisht", "run", "test/test"]).unwrap();
        let no_run_opts = parse_from_argv(vec!["gisht", "test/test"]).unwrap();
        assert_eq!(run_opts, no_run_opts);
    }

    /// Verify that `run` subcommand is optional when running a gist with args.
    #[test]
    fn run_optional_with_args() {
        let run_opts = parse_from_argv(vec![
            "gisht", "run", "test/test", "--", "some", "arg"]).unwrap();
        let no_run_opts = parse_from_argv(vec![
            "gisht", "test/test", "--", "some", "arg"]).unwrap();
        assert_eq!(run_opts, no_run_opts);
    }

    /// Verify that args can only be provided for the `run` subcommand.
    #[test]
    fn gist_args_only_for_run() {
        for cmd in Command::iter_variants().filter(|cmd| *cmd != Command::Run) {
            let args = vec!["gisht", cmd.name(), "test/test", "--", "some", "arg"];
            assert!(parse_from_argv(args).is_err(),
                "Command `{}` unexpectedly accepted arguments!", cmd.name());
        }
    }

    /// Verify that passing an invalid gist spec will cause an error.
    #[test]
    fn invalid_gist() {
        let gist_uri = "foo:foo:foo";  // Invalid.
        let args = vec!["gisht", "run", gist_uri];
        assert!(parse_from_argv(args).is_err(),
            "Gist URI `{}` should cause a parse error but didn't", gist_uri);
    }
}
