//! The highlight of `riff chat` and `riff tail`, as in IRC (#1580): a
//! line for the person at the terminal stands out. The client finds it,
//! and the server and the selectors do not change
//! (01M6XSHCZB0Y08XRJ3J16Q4RTF).
//!
//! [`why`] reads a line. It is a highlight when it names the person
//! (`@USER` anywhere, or `USER:` at the start), when it is a direct line
//! to the person, when it holds a word of the list of the person, or when
//! it is an ask of riff to the person (01M6XSHMGM82QSABRWCYH595VH). A
//! line that the person typed is no highlight.
//!
//! The table `[chat]` of the settings says what a highlight does
//! ([`Config`], [`crate::settings::chat`], 01M6XSHMK5QPP73CV98F8BHRRT).
//! [`Highlights`] does it for one chat or tail: the bold of the line,
//! and the marks for the terminal after it (01M6XSHMNNTRC66CMZ0DF2X5RA).
//! The hook [`run_hook`] runs on its own (01M6XSHMR46J353DK9HKSHVN0P). In
//! the quiet time, the bold and the title stay
//! (01M6XSHMTNBVMMGNBCSRCSQDYJ). A line of the history at the start of
//! `riff chat` is only bold (01M6XSHMX4VJKQHAHP8H4XDSBQ).
//!
//! ```mermaid
//! flowchart LR
//!     L["a line"] --> W{"why"}
//!     W -- "no highlight" --> P["the line as it is"]
//!     W -- "name, direct, word, ask" --> B["bold, unless no color"]
//!     B --> H{"a new line?"}
//!     H -- "the history" --> E["no more"]
//!     H -- "new" --> T["the title: (N) riff chat"]
//!     T --> Q{"the quiet time?"}
//!     Q -- yes --> E
//!     Q -- no --> R["the bell, the toast, the hook"]
//! ```

use std::io::Write as _;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail};
use chrono::NaiveTime;
use riff_client::threads::{BY_RIFF, Line, Names, action_text};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::Kind;
use tokio::io::AsyncWriteExt;

use crate::chat::said;
use crate::text::safe;

/// Why a line is a highlight for the person at the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The line names the person: `@USER` anywhere, or `USER:` at the
    /// start.
    Name,
    /// A line of the other member of a direct thread of the person, or a
    /// line to the terminals of the person (`person=true`).
    Direct,
    /// The line holds this word of the list `chat.highlight`.
    Word(String),
    /// A line of riff, with the kind `riff` of riff-server or the mark
    /// [`BY_RIFF`], that names the person or goes to the terminals of the
    /// person: for example "riff release v4.8.0 waits for your approval".
    Ask,
}

/// Why `line` of `thread` is a highlight for `me`, the person at the
/// terminal, with the words `words` of the person. `None` for a line
/// that is no highlight, and for each line that the person typed. A name
/// and a word match with no regard to case, and a word only as a whole
/// word.
///
/// ```
/// use riff::highlight::{Why, why};
/// use riff_client::api::Checked;
/// use riff_client::threads::Line;
/// use riff_core::wire::{Kind, Message};
///
/// let me = "riff://mike@thelio".parse()?;
/// let chat = "chat".parse()?;
/// let line = |body: &str| {
///     let message = Message {
///         seq: 1,
///         from: "riff://brett@heron".parse().unwrap(),
///         to: vec![],
///         body: body.into(),
///         at_ms: 0,
///         kind: Kind::Message,
///         sig: None,
///         payload: None,
///     };
///     Line::of(&Checked { message, verified: true }, &chat)
/// };
/// let words = ["release".to_owned()];
/// let why = |body: &str| why(&line(body), &chat, &me, &words);
/// assert_eq!(why("can @Mike look at #12?"), Some(Why::Name));
/// assert_eq!(why("mike: look at #12"), Some(Why::Name));
/// assert_eq!(why("is the release out?"), Some(Why::Word("release".into())));
/// assert_eq!(why("@brett: the released build"), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn why(line: &Line, thread: &ThreadName, me: &SessionUri, words: &[String]) -> Option<Why> {
    let user = me.who().user();
    let body = line.body.strip_prefix(BY_RIFF);
    let by_riff = line.kind == Kind::Riff || body.is_some();
    let from = line.from.who();
    if !by_riff && from.session().is_none() && from.user() == user {
        return None;
    }
    let body = body.unwrap_or(&line.body);
    let text = action_text(body).unwrap_or(body);
    let to_me = line
        .to
        .iter()
        .any(|to| to.selector.person == Some(true) && to.selector.matches(me));
    let named = names(text, user);
    if by_riff && (named || to_me) {
        return Some(Why::Ask);
    }
    if to_me || thread.peer(me.who()).is_some_and(|peer| *from == peer) {
        return Some(Why::Direct);
    }
    if named {
        return Some(Why::Name);
    }
    words
        .iter()
        .find(|word| holds(text, word))
        .map(|word| Why::Word(word.clone()))
}

/// True when `text` names `user`: `@USER` anywhere, as
/// [`riff_client::chat::names`] reads a name, or `USER:` at the start.
fn names(text: &str, user: &str) -> bool {
    let start = text
        .split_whitespace()
        .next()
        .and_then(|word| word.strip_suffix(':'))
        .is_some_and(|name| name.eq_ignore_ascii_case(user));
    start || riff_client::chat::names(text).any(|name| name.eq_ignore_ascii_case(user))
}

/// True when `text` holds `word` as a whole word, with no regard to
/// case. An empty word is in no text.
fn holds(text: &str, word: &str) -> bool {
    let (text, word) = (text.to_lowercase(), word.trim().to_lowercase());
    !word.is_empty()
        && text.match_indices(&word).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
}

/// What a highlight does: the table `[chat]` of the settings
/// (01M6XSHMK5QPP73CV98F8BHRRT). [`Default`] gives the defaults: bold
/// and the bell, and nothing more.
///
/// ```
/// let config = riff::highlight::Config::default();
/// assert!(config.words.is_empty());
/// assert!(config.bold && config.bell);
/// assert!(!config.title && !config.toast);
/// assert_eq!((config.hook, config.quiet), (None, None));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `highlight`: the words of the person. The name of the person and
    /// the direct lines are always a highlight.
    pub words: Vec<String>,
    /// `bold`: the line in bold.
    pub bold: bool,
    /// `bell`: the terminal bell (BEL).
    pub bell: bool,
    /// `title`: the number of the highlights in the title of the
    /// terminal, for example `(2) riff chat`.
    pub title: bool,
    /// `toast`: a desktop toast through the terminal (OSC 9 or OSC 777).
    pub toast: bool,
    /// `on_highlight`: the hook command. `None` for an empty text.
    pub hook: Option<String>,
    /// `quiet`: the quiet time. `None` for an empty text.
    pub quiet: Option<Quiet>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            words: Vec::new(),
            bold: true,
            bell: true,
            title: false,
            toast: false,
            hook: None,
            quiet: None,
        }
    }
}

/// Where the lines go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Out {
    /// True when the lines have color: not with `--color never`, not with
    /// `NO_COLOR`, and not in a pipe without `--color always`.
    pub color: bool,
    /// True when stdout is a terminal. The bell, the title and the toast
    /// go only to a terminal.
    pub terminal: bool,
}

impl Out {
    /// Where the lines of this process go: the color of stdout, as
    /// `anstream` picks it from `--color`, `NO_COLOR` and
    /// `CLICOLOR_FORCE`.
    pub fn stdout() -> Out {
        use std::io::IsTerminal;
        Out {
            color: anstream::AutoStream::choice(&std::io::stdout()) != anstream::ColorChoice::Never,
            terminal: std::io::stdout().is_terminal(),
        }
    }
}

/// What one highlight does ([`Config::actions`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Actions {
    pub bold: bool,
    pub bell: bool,
    pub title: bool,
    pub toast: bool,
    pub hook: bool,
}

impl Config {
    /// What a highlight at the local time `at` does, with the lines to
    /// `out`. A line of the history that `riff chat` shows at its start
    /// is not `live`: it is only bold (01M6XSHMX4VJKQHAHP8H4XDSBQ). In the
    /// quiet time, the bold and the title stay (01M6XSHMTNBVMMGNBCSRCSQDYJ).
    ///
    /// ```
    /// use chrono::NaiveTime;
    /// use riff::highlight::{Actions, Config, Out};
    ///
    /// let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    /// let tty = Out { color: true, terminal: true };
    /// let on = Actions { bold: true, bell: true, ..Actions::default() };
    /// assert_eq!(Config::default().actions(noon, tty, true), on);
    /// // No color: no bold. A pipe: no bell.
    /// let pipe = Out { color: false, terminal: false };
    /// assert_eq!(Config::default().actions(noon, pipe, true), Actions::default());
    /// ```
    pub fn actions(&self, at: NaiveTime, out: Out, live: bool) -> Actions {
        let loud = live && !self.quiet.is_some_and(|quiet| quiet.holds(at));
        Actions {
            bold: self.bold && out.color,
            bell: self.bell && out.terminal && loud,
            title: self.title && out.terminal && live,
            toast: self.toast && out.terminal && loud,
            hook: self.hook.is_some() && loud,
        }
    }
}

/// A quiet time of each day, for example `22:00-07:00`
/// (01M6XSHMTNBVMMGNBCSRCSQDYJ). It holds from its start up to its end.
/// A quiet time that ends before its start goes over midnight.
///
/// ```
/// use chrono::NaiveTime;
/// use riff::highlight::Quiet;
///
/// let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
/// let night: Quiet = "22:00-07:00".parse()?;
/// assert!(night.holds(at(23, 30)) && night.holds(at(6, 59)));
/// assert!(!night.holds(at(7, 0)) && !night.holds(at(12, 0)));
/// let lunch: Quiet = "12:00-13:00".parse()?;
/// assert!(lunch.holds(at(12, 0)) && !lunch.holds(at(13, 0)));
/// assert!("22:00".parse::<Quiet>().is_err());
/// assert!("22:00-25:00".parse::<Quiet>().is_err());
/// assert!("08:00-08:00".parse::<Quiet>().is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quiet {
    start: NaiveTime,
    end: NaiveTime,
}

impl Quiet {
    /// True when the quiet time holds at the local time `at`.
    pub fn holds(&self, at: NaiveTime) -> bool {
        match self.start < self.end {
            true => self.start <= at && at < self.end,
            false => at >= self.start || at < self.end,
        }
    }
}

impl FromStr for Quiet {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Quiet> {
        let bad = || format!("{text:?} is not a time such as 22:00-07:00");
        let (start, end) = text.split_once('-').with_context(bad)?;
        let time = |t: &str| NaiveTime::parse_from_str(t.trim(), "%H:%M").with_context(bad);
        let (start, end) = (time(start)?, time(end)?);
        if start == end {
            bail!("{text:?} starts and ends at the same time");
        }
        Ok(Quiet { start, end })
    }
}

/// The terminal bell.
pub const BELL: char = '\x07';

/// The highlights of one `riff chat` or `riff tail` of `me`.
pub struct Highlights {
    me: SessionUri,
    config: Config,
    out: Out,
    title: Title,
    /// True for a toast through OSC 777, false for OSC 9.
    osc777: bool,
}

/// What a line gets ([`Highlights::line`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lit {
    /// True for a line in bold: [`bold`].
    pub bold: bool,
    /// The escape codes for the terminal after the line: the bell, the
    /// title and the toast. The line goes out with no escape code when
    /// it has no color, but these go out as they are.
    pub marks: String,
}

impl Lit {
    /// Writes the marks to stdout as they are, after the line: not
    /// through `anstream`, which removes each escape code with no color.
    pub fn put(&self) {
        if self.marks.is_empty() {
            return;
        }
        let mut out = std::io::stdout();
        let _ = out.write_all(self.marks.as_bytes());
        let _ = out.flush();
    }
}

impl Highlights {
    /// The highlights of `me` with `config`. `label` is the name of the
    /// program in the title of the terminal: `riff chat`, or
    /// `riff tail THREAD`.
    pub fn new(me: &SessionUri, config: Config, label: &str, out: Out) -> Highlights {
        Highlights {
            me: me.clone(),
            config,
            out,
            title: Title::new(label),
            osc777: osc777(
                std::env::var_os("VTE_VERSION").is_some(),
                &std::env::var("TERM").unwrap_or_default(),
            ),
        }
    }

    /// The title of the terminal, for the line editor of `riff chat`: a
    /// key clears its count ([`Title::clear`]).
    pub fn title(&self) -> Title {
        self.title.clone()
    }

    /// What `line` of `thread` gets now, with the names `names` of
    /// riff-server for its sender. A line of the history at the
    /// start of `riff chat` is not `live`. For a new highlight, it starts
    /// the hook ([`run_hook`]): a hook that fails gives a line of riff
    /// ([`crate::chat::notice`]).
    pub fn line(&self, line: &Line, names: &Names, thread: &ThreadName, live: bool) -> Lit {
        let shown = self.line_at(line, names, thread, live, chrono::Local::now().time());
        let hook = self.config.hook.as_ref();
        if let Some(hook) = hook.filter(|_| shown.hook) {
            let (hook, sender) = (hook.clone(), said(line, names).name);
            let (text, thread) = (plain(line, names), thread.to_string());
            tokio::spawn(async move {
                if let Err(e) = run_hook(&hook, &text, &sender, &thread).await {
                    crate::chat::notice(&safe(&format!("riff: on_highlight failed: {e:#}")));
                }
            });
        }
        shown.lit
    }

    /// What `line` of `thread` gets at the local time `at`, and whether
    /// the hook runs.
    fn line_at(
        &self,
        line: &Line,
        names: &Names,
        thread: &ThreadName,
        live: bool,
        at: NaiveTime,
    ) -> Shown {
        if why(line, thread, &self.me, &self.config.words).is_none() {
            return Shown::default();
        }
        let actions = self.config.actions(at, self.out, live);
        let mut marks = String::new();
        if actions.bell {
            marks.push(BELL);
        }
        if actions.title {
            marks.push_str(&self.title.mark());
        }
        if actions.toast {
            marks.push_str(&toast(&self.title.label, &plain(line, names), self.osc777));
        }
        Shown {
            lit: Lit {
                bold: actions.bold,
                marks,
            },
            hook: actions.hook,
        }
    }
}

/// What [`Highlights::line_at`] gives.
#[derive(Debug, Default, PartialEq, Eq)]
struct Shown {
    lit: Lit,
    hook: bool,
}

/// The line as the chat shows it, with the names `names` of riff-server
/// and with no time and no style: `<NAME> TEXT`, `[USER ID] TEXT` or `*
/// NAME TEXT` ([`said`]). The hook gets it on stdin, and the toast shows
/// it.
///
/// ```
/// use riff_client::api::Checked;
/// use riff_client::threads::{Line, Names};
/// use riff_core::wire::{Kind, Message};
///
/// let message = Message {
///     seq: 1,
///     from: "riff://brett@heron".parse()?,
///     to: vec![],
///     body: "mike: look\x1b[2J at #12".into(),
///     at_ms: 0,
///     kind: Kind::Message,
///     sig: None,
///     payload: None,
/// };
/// let line = Line::of(&Checked { message, verified: true }, &"chat".parse()?);
/// assert_eq!(riff::highlight::plain(&line, &Names::default()), "<brett> mike: look at #12");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn plain(line: &Line, names: &Names) -> String {
    let said = said(line, names);
    format!("{}{}{} {}", said.before, said.name, said.after, said.body)
}

/// `line` in bold, also each part of it after the end of a style in it.
/// `shown` is `line` with a date line before it, or `line` itself: the
/// date line stays as it is.
///
/// ```
/// use riff::style::{DIM, styled};
///
/// let line = format!("{} hi", styled(DIM, "14:02"));
/// let shown = format!("2026-09-28\n{line}");
/// assert_eq!(
///     riff::highlight::bold(&shown, &line),
///     "2026-09-28\n\x1b[1m\x1b[2m14:02\x1b[0m\x1b[1m hi\x1b[0m"
/// );
/// ```
pub fn bold(shown: &str, line: &str) -> String {
    let date = shown.strip_suffix(line).unwrap_or_default();
    let (on, reset) = (crate::style::BOLD.render().to_string(), "\x1b[0m");
    let line = line.replace(reset, &format!("{reset}{on}"));
    format!("{date}{on}{line}{reset}")
}

/// The title of the terminal: the label, and the number of the
/// highlights since the last key (01M6XSHMNNTRC66CMZ0DF2X5RA). Each clone
/// shares the number.
///
/// ```
/// let title = riff::highlight::Title::new("riff chat");
/// assert_eq!(title.clear(), None);
/// assert_eq!(title.mark(), "\x1b]0;(1) riff chat\x07");
/// assert_eq!(title.clone().mark(), "\x1b]0;(2) riff chat\x07");
/// assert_eq!(title.clear().as_deref(), Some("\x1b]0;riff chat\x07"));
/// assert_eq!(title.clear(), None);
/// ```
#[derive(Debug, Clone)]
pub struct Title {
    label: Arc<str>,
    count: Arc<AtomicUsize>,
}

impl Title {
    /// A title with the label `label` and no highlight.
    pub fn new(label: &str) -> Title {
        Title {
            label: safe(label).into(),
            count: Arc::default(),
        }
    }

    /// One more highlight: the escape code that sets the title to
    /// `(N) LABEL`.
    pub fn mark(&self) -> String {
        let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        set_title(&format!("({n}) {}", self.label))
    }

    /// A key, or a new prompt: the escape code that sets the title back
    /// to the label. `None` when the title has no number.
    pub fn clear(&self) -> Option<String> {
        (self.count.swap(0, Ordering::SeqCst) > 0).then(|| set_title(&self.label))
    }
}

/// The escape code that sets the title of the terminal (OSC 0).
fn set_title(text: &str) -> String {
    format!("\x1b]0;{text}{BELL}")
}

/// True for a toast through OSC 777: in a terminal of VTE (`VTE_VERSION`
/// is set), foot or rxvt. Else the toast goes through OSC 9.
///
/// ```
/// use riff::highlight::osc777;
///
/// assert!(osc777(true, "xterm-256color"));
/// assert!(osc777(false, "foot") && osc777(false, "rxvt-unicode-256color"));
/// assert!(!osc777(false, "xterm-kitty") && !osc777(false, ""));
/// ```
pub fn osc777(vte: bool, term: &str) -> bool {
    vte || term.starts_with("foot") || term.starts_with("rxvt")
}

/// The escape code of a desktop toast with the line `text`: OSC 777 with
/// the title `label`, or OSC 9 with the text alone. The text is one safe
/// line ([`crate::text::forge`]).
///
/// ```
/// use riff::highlight::toast;
///
/// assert_eq!(toast("riff chat", "<b@h> hi\n\x07you", false), "\x1b]9;<b@h> hi you\x07");
/// assert_eq!(toast("riff chat", "<b@h> a; b", true), "\x1b]777;notify;riff chat;<b@h> a, b\x07");
/// ```
pub fn toast(label: &str, text: &str, osc777: bool) -> String {
    let text = crate::text::forge(text);
    match osc777 {
        true => format!("\x1b]777;notify;{label};{}{BELL}", text.replace(';', ",")),
        false => format!("\x1b]9;{text}{BELL}"),
    }
}

/// Runs the hook `command` with `sh -c` for the line `text`
/// (01M6XSHMR46J353DK9HKSHVN0P). The command gets the line on stdin, the
/// sender in `RIFF_SENDER` and the thread in `RIFF_THREAD`. Its output
/// goes nowhere. It ends when the command ends: an exit that is not 0 is
/// an error.
pub async fn run_hook(command: &str, text: &str, sender: &str, thread: &str) -> Result<()> {
    let mut child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("RIFF_SENDER", sender)
        .env("RIFF_THREAD", thread)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start sh")?;
    if let Some(mut stdin) = child.stdin.take() {
        // A command that reads no stdin closes it: that is no error.
        let _ = stdin.write_all(format!("{text}\n").as_bytes()).await;
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("{command}: {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use riff_client::api::Checked;
    use riff_core::name::Who;
    use riff_core::wire::Message;

    use super::*;

    fn me() -> SessionUri {
        "riff://mike@thelio".parse().unwrap()
    }

    fn chat() -> ThreadName {
        ThreadName::chat()
    }

    /// A line of `from` to `to` in `thread`.
    fn line_in(thread: &ThreadName, from: &str, to: &[&str], kind: Kind, body: &str) -> Line {
        let message = Message {
            seq: 7,
            from: from.parse().unwrap(),
            to: to.iter().map(|s| s.parse().unwrap()).collect(),
            body: body.into(),
            at_ms: 0,
            kind,
            sig: None,
            payload: None,
        };
        Line::of(
            &Checked {
                message,
                verified: true,
            },
            thread,
        )
    }

    /// A chat line of brett.
    fn brett(body: &str) -> Line {
        line_in(&chat(), "riff://brett@heron", &[], Kind::Message, body)
    }

    fn why_of(line: &Line) -> Option<Why> {
        why(
            line,
            &chat(),
            &me(),
            &["release".to_owned(), "go live".to_owned()],
        )
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    const TTY: Out = Out {
        color: true,
        terminal: true,
    };

    #[test]
    fn a_name_anywhere_is_a_highlight() {
        assert_eq!(why_of(&brett("can @mike look at #12?")), Some(Why::Name));
        assert_eq!(why_of(&brett("ask @MIKE, please")), Some(Why::Name));
        assert_eq!(why_of(&brett("@mike:riff is #12 done?")), Some(Why::Name));
        assert_eq!(why_of(&brett("/me pings @mike")), Some(Why::Name));
        // Another name, or the name with no @, is none.
        assert_eq!(why_of(&brett("can @mikey look?")), None);
        assert_eq!(why_of(&brett("mail mike@thelio")), None);
        assert_eq!(why_of(&brett("@lead is #12 done?")), None);
    }

    #[test]
    fn the_name_and_a_colon_at_the_start_is_a_highlight() {
        assert_eq!(why_of(&brett("mike: look at #12")), Some(Why::Name));
        assert_eq!(why_of(&brett("Mike: look")), Some(Why::Name));
        // Not at the start, or with no colon: none.
        assert_eq!(why_of(&brett("look, mike: #12")), None);
        assert_eq!(why_of(&brett("mike look at #12")), None);
    }

    #[test]
    fn a_line_of_the_other_member_of_a_direct_thread_is_a_highlight() {
        let mike = Who::new("mike", None).unwrap();
        let brett_who = Who::new("brett", None).unwrap();
        let direct = ThreadName::direct(&mike, &brett_who);
        let from_brett = line_in(&direct, "riff://brett@heron", &[], Kind::Message, "hi");
        assert_eq!(why(&from_brett, &direct, &me(), &[]), Some(Why::Direct));
        // A direct thread of two others is none of mine.
        let ann = Who::new("ann", None).unwrap();
        let others = ThreadName::direct(&ann, &brett_who);
        let theirs = line_in(&others, "riff://brett@heron", &[], Kind::Message, "hi");
        assert_eq!(why(&theirs, &others, &me(), &[]), None);
    }

    #[test]
    fn a_line_to_the_terminals_of_the_person_is_a_direct_line() {
        let lead = "riff://mike@pangolin/o/riff?session=4e54d4e5&lead=true";
        let to = |to: &str| line_in(&chat(), lead, &[to], Kind::Message, "it is merged");
        assert_eq!(why_of(&to("user=mike,person=true")), Some(Why::Direct));
        assert_eq!(
            why_of(&to("user=mike,host=thelio,person=true")),
            Some(Why::Direct)
        );
        // The terminals of another host, or the agent sessions: none.
        assert_eq!(why_of(&to("user=mike,host=pangolin,person=true")), None);
        assert_eq!(why_of(&to("user=mike,lead=true")), None);
    }

    #[test]
    fn a_word_of_the_list_is_a_highlight_as_a_whole_word() {
        let release = Some(Why::Word("release".into()));
        assert_eq!(why_of(&brett("is the release out?")), release);
        assert_eq!(why_of(&brett("RELEASE now")), release);
        assert_eq!(why_of(&brett("(release)")), release);
        assert_eq!(
            why_of(&brett("we go live at 3")),
            Some(Why::Word("go live".into()))
        );
        // A part of a word is no word.
        assert_eq!(why_of(&brett("it is released")), None);
        assert_eq!(why_of(&brett("prerelease notes")), None);
        // An empty word is in no line.
        assert_eq!(why(&brett("hi"), &chat(), &me(), &[" ".into()]), None);
    }

    #[test]
    fn an_ask_of_riff_to_the_person_is_a_highlight() {
        let person = "riff://mike@pangolin";
        let waits = "@mike riff release v3.3.0 waits for your approval: https://x/runs/9.";
        let ask = |to: &[&str], kind, body: &str| {
            why(
                &line_in(&chat(), person, to, kind, body),
                &chat(),
                &me(),
                &[],
            )
        };
        assert_eq!(ask(&[], Kind::Riff, waits), Some(Why::Ask));
        // A line that riff writes as the person has the mark
        // (01M6Y4ZCTSTVM971BDC1F7GJPK).
        let marked = format!("{BY_RIFF}{waits}");
        assert_eq!(ask(&[], Kind::Message, &marked), Some(Why::Ask));
        // The question of a lead goes to the terminals of the person.
        let question = "como-technologies/riff #12 asks you a question: Which design?";
        let to_me = ["user=mike,person=true"];
        assert_eq!(ask(&to_me, Kind::Riff, question), Some(Why::Ask));
        // A line of riff for another person is none.
        let theirs = "@brett riff release v3.3.0 waits for your approval";
        assert_eq!(ask(&[], Kind::Riff, theirs), None);
    }

    #[test]
    fn another_line_is_no_highlight() {
        assert_eq!(why_of(&brett("hello all")), None);
        assert_eq!(why_of(&brett("")), None);
        // A lead of the person that names no one.
        let lead = "riff://mike@thelio/o/riff?session=4e54d4e5&lead=true";
        let line = line_in(
            &chat(),
            lead,
            &["user=mike,lead=true"],
            Kind::Message,
            "done",
        );
        assert_eq!(why_of(&line), None);
    }

    #[test]
    fn a_line_that_the_person_typed_is_no_highlight() {
        let own = |from: &str| line_in(&chat(), from, &[], Kind::Message, "@mike: release now");
        assert_eq!(why_of(&own("riff://mike@thelio")), None);
        assert_eq!(why_of(&own("riff://mike@pangolin")), None);
        // A session of the person is not the person.
        let lead = "riff://mike@thelio/o/riff?session=4e54d4e5&lead=true";
        assert_eq!(why_of(&own(lead)), Some(Why::Name));
    }

    #[test]
    fn the_defaults_are_bold_and_the_bell() {
        let actions = Config::default().actions(at(12, 0), TTY, true);
        assert_eq!(
            actions,
            Actions {
                bold: true,
                bell: true,
                title: false,
                toast: false,
                hook: false,
            }
        );
    }

    #[test]
    fn each_action_goes_on_with_its_key() {
        let all = Config {
            title: true,
            toast: true,
            hook: Some("true".into()),
            ..Config::default()
        };
        let on = Actions {
            bold: true,
            bell: true,
            title: true,
            toast: true,
            hook: true,
        };
        assert_eq!(all.actions(at(12, 0), TTY, true), on);
        let none = Config {
            bold: false,
            bell: false,
            ..Config::default()
        };
        assert_eq!(none.actions(at(12, 0), TTY, true), Actions::default());
    }

    #[test]
    fn no_color_turns_off_the_bold_and_a_pipe_gets_no_marks() {
        let all = Config {
            title: true,
            toast: true,
            hook: Some("true".into()),
            ..Config::default()
        };
        // `--color never` and `NO_COLOR`: no color.
        let never = Out {
            color: false,
            terminal: true,
        };
        let actions = all.actions(at(12, 0), never, true);
        assert!(!actions.bold && actions.bell && actions.title && actions.toast);
        // A pipe gets no bell, no title and no toast; the hook runs.
        let pipe = Out {
            color: false,
            terminal: false,
        };
        let actions = all.actions(at(12, 0), pipe, true);
        assert_eq!(
            actions,
            Actions {
                hook: true,
                ..Actions::default()
            }
        );
    }

    #[test]
    fn the_quiet_time_keeps_the_marks_and_stops_the_bell_the_toast_and_the_hook() {
        let config = Config {
            title: true,
            toast: true,
            hook: Some("true".into()),
            quiet: Some("22:00-07:00".parse().unwrap()),
            ..Config::default()
        };
        let quiet = Actions {
            bold: true,
            title: true,
            ..Actions::default()
        };
        assert_eq!(config.actions(at(23, 0), TTY, true), quiet);
        assert_eq!(config.actions(at(3, 0), TTY, true), quiet);
        let loud = config.actions(at(7, 0), TTY, true);
        assert!(loud.bell && loud.toast && loud.hook);
    }

    #[test]
    fn a_line_of_the_history_is_only_bold() {
        let config = Config {
            title: true,
            toast: true,
            hook: Some("true".into()),
            ..Config::default()
        };
        let only_bold = Actions {
            bold: true,
            ..Actions::default()
        };
        assert_eq!(config.actions(at(12, 0), TTY, false), only_bold);
    }

    fn highlights(config: Config, out: Out) -> Highlights {
        Highlights::new(&me(), config, "riff chat", out)
    }

    /// What `line` of the chat gets from `h` at noon, live, with no names
    /// of riff-server.
    fn at_noon(h: &Highlights, line: &Line) -> Shown {
        h.line_at(line, &Names::default(), &chat(), true, at(12, 0))
    }

    #[test]
    fn a_highlight_gets_the_bell_and_the_title_after_the_line() {
        let config = Config {
            title: true,
            ..Config::default()
        };
        let h = highlights(config, TTY);
        let shown = at_noon(&h, &brett("mike: look"));
        assert!(shown.lit.bold);
        assert_eq!(shown.lit.marks, "\x07\x1b]0;(1) riff chat\x07");
        assert!(!shown.hook);
        let next = at_noon(&h, &brett("@mike and #13"));
        assert_eq!(next.lit.marks, "\x07\x1b]0;(2) riff chat\x07");
        // A key clears the number.
        assert_eq!(h.title().clear().as_deref(), Some("\x1b]0;riff chat\x07"));
        // Another line gets nothing.
        let other = at_noon(&h, &brett("hello all"));
        assert_eq!(other, Shown::default());
    }

    #[test]
    fn a_highlight_gets_the_toast_and_the_hook_with_their_keys() {
        let config = Config {
            bell: false,
            toast: true,
            hook: Some("cat".into()),
            ..Config::default()
        };
        let mut h = highlights(config, TTY);
        h.osc777 = false;
        let shown = at_noon(&h, &brett("mike: look"));
        assert_eq!(shown.lit.marks, "\x1b]9;<brett> mike: look\x07");
        assert!(shown.hook);
        // An ask of riff shows its real sender, the person, and the mark
        // (01M6Y4ZCTSTVM971BDC1F7GJPK).
        let waits = format!("{BY_RIFF}@mike the deploy waits for your approval");
        let ask = line_in(&chat(), "riff://mike@pangolin", &[], Kind::Message, &waits);
        let shown = at_noon(&h, &ask);
        assert_eq!(
            shown.lit.marks,
            "\x1b]9;<mike> [by riff] @mike the deploy waits for your approval\x07"
        );
    }

    #[test]
    fn the_bold_keeps_each_part_of_a_styled_line_bold() {
        let line = crate::style::styled(crate::style::DIM, "14:02");
        let shown = bold(&line, &line);
        assert_eq!(shown, "\x1b[1m\x1b[2m14:02\x1b[0m\x1b[1m\x1b[0m");
        // Through a stream with no color, no escape code is left.
        let plain = anstream::adapter::strip_str(&shown).to_string();
        assert_eq!(plain, "14:02");
    }

    #[tokio::test]
    async fn the_hook_gets_the_line_on_stdin_and_the_sender_and_the_thread() {
        let dir = tempfile::tempdir().unwrap();
        let (stdin, env) = (dir.path().join("stdin"), dir.path().join("env"));
        let hook = format!(
            "cat > '{}'; printf '%s|%s' \"$RIFF_SENDER\" \"$RIFF_THREAD\" > '{}'",
            stdin.display(),
            env.display()
        );
        let line = brett("mike: look at #12");
        let names = Names::default();
        let said = said(&line, &names);
        run_hook(&hook, &plain(&line, &names), &said.name, "chat")
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&stdin).unwrap(),
            "<brett> mike: look at #12\n"
        );
        assert_eq!(std::fs::read_to_string(&env).unwrap(), "brett|chat");
    }

    #[tokio::test]
    async fn a_hook_that_fails_is_an_error() {
        let error = run_hook("exit 3", "a line", "brett@heron", "chat")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("exit 3"), "{error:#}");
        // A hook that reads no stdin is no error.
        run_hook("true", &"x".repeat(1 << 20), "brett@heron", "chat")
            .await
            .unwrap();
    }
}
