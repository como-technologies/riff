//! The layout of `riff --help` (01M3NJDSQ23FFRMH8ZD4GC57WY).
//!
//! clap 4 lists subcommands in one flat list. [`grouped`] gives a
//! command a help template that lists its subcommands under
//! headings, in the order of [`GROUPS`]. Each line shows the name and
//! the short help (`about`) of one subcommand. The long help of each
//! subcommand stays for `riff help CMD`.
//!
//! A hidden subcommand, for example the plumbing that only the plugin
//! runs, is in no group. `riff help CMD` still shows it.
//!
//! ```
//! use clap::Command;
//! use riff::help::{Group, grouped};
//!
//! let cmd = Command::new("tool")
//!     .about("A tool")
//!     .subcommand(Command::new("go").about("Go now"))
//!     .subcommand(Command::new("stop").about("Stop at once"))
//!     .subcommand(Command::new("hook").about("Plumbing").hide(true));
//! let groups = [Group { heading: "Move", commands: &["go", "stop"] }];
//! let help = grouped(cmd, &groups).render_help().to_string();
//! assert!(help.contains("Move:\n  go    Go now\n  stop  Stop at once\n"));
//! assert!(!help.contains("hook"));
//! ```

use std::ffi::OsString;
use std::fmt::Write;
use std::io::IsTerminal;

use clap::builder::StyledStr;
use clap::error::ErrorKind;
use clap::{ArgMatches, Command};

/// The widest line of help, in columns. Help wraps at this width, also
/// in a wider terminal.
pub const WIDTH: usize = 80;

/// One heading of `riff --help`, with the subcommands under it.
#[derive(Debug)]
pub struct Group {
    /// The heading, with no colon.
    pub heading: &'static str,
    /// The names of the subcommands, in the order of the help.
    pub commands: &'static [&'static str],
}

/// The groups of `riff --help`, in order. Each subcommand that a person
/// uses is in one group.
pub const GROUPS: &[Group] = &[
    Group {
        heading: "Get started",
        commands: &[
            "connect", "enable", "disable", "setup", "login", "logout", "update", "server",
        ],
    },
    Group {
        heading: "Work in the riff",
        commands: &[
            "who", "whoami", "top", "read", "tail", "chat", "post", "tell", "status", "claim",
            "release",
        ],
    },
    Group {
        heading: "Pull requests",
        commands: &["pr", "verify", "usage"],
    },
    Group {
        heading: "Lead",
        commands: &["lead", "pause", "resume", "workers", "worktrees"],
    },
    Group {
        heading: "Members",
        commands: &["members", "invite", "remove", "admin", "owner"],
    },
];

/// Give `cmd` a help template that lists its subcommands under the
/// headings of `groups`, and wrap its help at [`WIDTH`]. A name in
/// `groups` that is not a subcommand of `cmd` shows nothing.
pub fn grouped(cmd: Command, groups: &[Group]) -> Command {
    let styles = cmd.get_styles();
    let (header, literal) = (styles.get_header(), styles.get_literal());
    let width = groups
        .iter()
        .flat_map(|group| group.commands)
        .map(|name| name.len())
        .max()
        .unwrap_or(0);
    let mut template = StyledStr::new();
    let _ = write!(
        template,
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n"
    );
    for group in groups {
        let _ = write!(template, "\n{header}{}:{header:#}\n", group.heading);
        for name in group.commands {
            let Some(sub) = cmd.find_subcommand(name) else {
                continue;
            };
            let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
            let _ = writeln!(template, "  {literal}{name:<width$}{literal:#}  {about}");
        }
    }
    let _ = write!(
        template,
        "\n{header}Options:{header:#}\n{{options}}\n\n\
         Run '{literal}{name} help <command>{literal:#}' for more about a command.\n",
        name = cmd.get_name()
    );
    cmd.help_template(template).max_term_width(WIDTH)
}

/// Parse the arguments of this process with `cmd`. See [`try_matches`].
/// With no argument at all, show the help on stderr and exit with 2.
pub fn matches(mut cmd: Command) -> ArgMatches {
    if std::env::args_os().len() == 1 {
        let help = cmd.render_help();
        if std::io::stderr().is_terminal() {
            eprint!("{}", help.ansi());
        } else {
            eprint!("{help}");
        }
        std::process::exit(2);
    }
    try_matches(cmd, std::env::args_os()).unwrap_or_else(|e| e.exit())
}

/// Parse `args` with `cmd`. With no subcommand, the error names no
/// subcommand, so that it names no hidden one
/// (01M3NT228WA11PGNWDJ0WP7PQD).
///
/// ```
/// use clap::Command;
///
/// let cmd = Command::new("tool")
///     .subcommand_required(true)
///     .arg(clap::Arg::new("server").long("server"))
///     .subcommand(Command::new("go"))
///     .subcommand(Command::new("hook").hide(true));
/// let e = riff::help::try_matches(cmd.clone(), ["tool", "--server", "x"]).unwrap_err();
/// assert_eq!(
///     e.to_string(),
///     "error: name a command. Run 'tool --help' to list them.\n"
/// );
/// assert!(riff::help::try_matches(cmd, ["tool", "go"]).is_ok());
/// ```
pub fn try_matches<I, T>(cmd: Command, args: I) -> Result<ArgMatches, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let name = cmd.get_name().to_owned();
    cmd.try_get_matches_from(args).map_err(|e| {
        if e.kind() == ErrorKind::MissingSubcommand {
            clap::Error::raw(
                e.kind(),
                format!("name a command. Run '{name} --help' to list them.\n"),
            )
        } else {
            e
        }
    })
}
