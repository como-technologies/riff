//! The styles of the output for people: `riff tail`, `riff who` and
//! `riff server` (01M3MEW73CDSJDSKX32XW80WZH). Each uses this one
//! module, so a session has the same color in each.
//!
//! The styles are ANSI escape codes. Print styled text through
//! `anstream`, which removes them when the output has no color
//! (01M3JDCA9070MY30AYHK3Y67EF, 01M3MEW75WC7Y4M1BKQ7SXRPNR).

use anstyle::{AnsiColor, Color, Style};
use riff_core::name::SessionUri;

/// The colors of the sessions. A session gets one of them from a hash
/// of its session ID ([`session`]). Red and yellow are for errors and
/// warnings, so they are not here.
pub const SESSION_COLORS: [AnsiColor; 8] = [
    AnsiColor::Green,
    AnsiColor::Blue,
    AnsiColor::Magenta,
    AnsiColor::Cyan,
    AnsiColor::BrightGreen,
    AnsiColor::BrightBlue,
    AnsiColor::BrightMagenta,
    AnsiColor::BrightCyan,
];

/// Dim text: a time, an age, a number, a URI, the mark `verified`.
pub const DIM: Style = Style::new().dimmed();

/// Text of less weight: the address of a message, `lead` and the claims
/// in `riff who`.
pub const MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

/// Bold text: `lead` in `riff tail`, `(you)` in `riff who`.
pub const BOLD: Style = Style::new().bold();

/// Good news: `live` in `riff who`.
pub const GOOD: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));

/// A warning. Also `paused` in `riff who`.
pub const WARNING: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));

/// An error. Also a blocked status in `riff who`.
pub const ERROR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

/// The style of a session: bold, with a color from a hash of its
/// session ID. So a session has the same color in each message of
/// `riff tail` and in `riff who`. A person (no session ID) is bold and
/// underlined, with no color.
///
/// ```
/// use riff::style;
///
/// let a = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let b = "riff://ann@heron/como-technologies/riff?session=a6cf".parse()?;
/// assert_eq!(style::session(&a), style::session(&b));
/// assert!(style::session(&a).get_fg_color().is_some());
/// let person = "riff://mike@pangolin".parse()?;
/// assert_eq!(style::session(&person), anstyle::Style::new().bold().underline());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn session(uri: &SessionUri) -> Style {
    match uri.who().session() {
        Some(id) => hashed(id),
        None => BOLD.underline(),
    }
}

/// The style of a person in `riff top`: bold, with a color from a hash
/// of the USER (01M3NT4M5D36KTZ5XZMDP6QFQT).
///
/// ```
/// use riff::style;
///
/// assert_eq!(style::person("mike"), style::person("mike"));
/// assert!(style::person("mike").get_fg_color().is_some());
/// ```
pub fn person(user: &str) -> Style {
    hashed(user)
}

/// Bold, with a color from a hash of `key`.
fn hashed(key: &str) -> Style {
    // FNV-1a: the same on each machine and each run.
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    let color = SESSION_COLORS[(hash % SESSION_COLORS.len() as u64) as usize];
    BOLD.fg_color(Some(Color::Ansi(color)))
}

/// `text` in `style`.
///
/// ```
/// use riff::style::{GOOD, styled};
///
/// let text = styled(GOOD, "live");
/// assert_eq!(text, "\x1b[32mlive\x1b[0m");
/// assert_eq!(anstream::adapter::strip_str(&text).to_string(), "live");
/// ```
pub fn styled(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sessions_mostly_get_different_colors() {
        let colors: std::collections::HashSet<_> = (0..100)
            .map(|i| {
                let uri: SessionUri = format!("riff://mike@pangolin?session={i:08x}-d54a")
                    .parse()
                    .unwrap();
                format!("{:?}", session(&uri).get_fg_color())
            })
            .collect();
        assert_eq!(colors.len(), SESSION_COLORS.len());
    }
}
