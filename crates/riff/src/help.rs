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

use std::fmt::Write;

use clap::Command;
use clap::builder::StyledStr;

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
        commands: &["connect", "login", "logout", "update", "server"],
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
        commands: &["pr", "verify"],
    },
    Group {
        heading: "Lead",
        commands: &["lead", "pause", "resume", "workers"],
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
