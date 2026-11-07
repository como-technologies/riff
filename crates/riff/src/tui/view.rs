//! The pure render of the TUI (01M6VAZKNXZV1RTG7MT0GMHKT3):
//! [`compute_view`] makes a [`View`] from the [`State`], the size and
//! the time, and [`render`] draws the view on a ratatui [`Buffer`].
//!
//! # Design
//!
//! A view is a list of rows, each a line at a place, the help popup,
//! and the places that a click hits ([`Hit`], 01M6VAZKG8FS4HEA9VSA4Z4YF1).
//! The size picks the [`Layout`] (01M6VAZKK4KHCDCX4RKG9B95D7):
//!
//! - [`Layout::Wide`], at 80 columns or more: the strip names each tab,
//!   and the other rows start after a margin of one column.
//! - [`Layout::Narrow`], under 80 columns: the strip is `‹ N/6 NAME ›`
//!   with no news, the header has only the state of the riff and the
//!   link, and the rows have no margin.
//! - [`Layout::Small`], under 40 columns or 10 rows: a line that asks
//!   for a bigger terminal.
//!
//! ```text
//! ‹ 1/6 Sessions ›
//! riff running · link ok 2s
//! the body of the tab
//!
//! the note
//! 1-6 tab  ? help  q quit
//! ```
//!
//! ```
//! use ratatui::buffer::Buffer;
//! use ratatui::layout::Rect;
//! use riff::tui::view::{compute_view, render};
//! use riff::tui::{Look, State};
//! use riff_core::build::Build;
//!
//! let mut state = State::new("0.1.0 10df8a4c0ffe 2026-11-06T12:00:00Z".parse().unwrap());
//! state.looked(Look::default(), 1_000);
//! let area = Rect::new(0, 0, 80, 24);
//! let view = compute_view(&state, area, 3_000);
//! let mut buffer = Buffer::empty(area);
//! render(&view, &mut buffer);
//! let row = |y: u16| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();
//! assert!(row(0).starts_with("[1 Sessions]  2 Chat  "), "{}", row(0));
//! assert!(row(1).starts_with(" riff running · build 10df8a4 · link ok 2s "), "{}", row(1));
//! ```

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Widget};
use riff_core::wire::RiffOwner;

use super::{Key, State, Tab, chat};
use crate::text;

/// The fewest columns of the wide layout.
pub const WIDE: u16 = 80;

/// The fewest columns of the TUI: under them, it asks for a bigger
/// terminal.
pub const LEAST_COLUMNS: u16 = 40;

/// The fewest rows of the TUI: under them, it asks for a bigger
/// terminal.
pub const LEAST_ROWS: u16 = 10;

/// The line of a terminal that is too small for the TUI.
pub const TOO_SMALL: &str =
    "The terminal is too small for riff. Make it 40 columns wide and 10 rows high. q ends riff.";

/// The layout of a size (01M6VAZKK4KHCDCX4RKG9B95D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// 80 columns or more.
    Wide,
    /// From 40 to 79 columns.
    Narrow,
    /// Under 40 columns or 10 rows: too small.
    Small,
}

impl Layout {
    /// The layout of `area`.
    ///
    /// ```
    /// use ratatui::layout::Rect;
    /// use riff::tui::view::Layout;
    ///
    /// assert_eq!(Layout::of(Rect::new(0, 0, 80, 24)), Layout::Wide);
    /// assert_eq!(Layout::of(Rect::new(0, 0, 200, 10)), Layout::Wide);
    /// assert_eq!(Layout::of(Rect::new(0, 0, 79, 24)), Layout::Narrow);
    /// assert_eq!(Layout::of(Rect::new(0, 0, 40, 24)), Layout::Narrow);
    /// assert_eq!(Layout::of(Rect::new(0, 0, 39, 24)), Layout::Small);
    /// assert_eq!(Layout::of(Rect::new(0, 0, 80, 9)), Layout::Small);
    /// ```
    pub fn of(area: Rect) -> Layout {
        if area.width < LEAST_COLUMNS || area.height < LEAST_ROWS {
            Layout::Small
        } else if area.width < WIDE {
            Layout::Narrow
        } else {
            Layout::Wide
        }
    }
}

/// A line of the view at its place.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The place: one row.
    pub area: Rect,
    /// The line.
    pub line: Line<'static>,
}

/// The help popup: the keys of the tab, in a box in the middle.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    /// The place of the box, with its border.
    pub area: Rect,
    /// The title on the border.
    pub title: String,
    /// The lines in the box.
    pub lines: Vec<Line<'static>>,
}

/// A place that a click hits, and the key that the click presses
/// (01M6VAZKG8FS4HEA9VSA4Z4YF1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The place.
    pub area: Rect,
    /// The key of the place.
    pub key: Key,
}

/// What [`render`] draws: the rows, the popup, and the places that a
/// click hits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    /// The rows of the screen.
    pub rows: Vec<Placed>,
    /// The help popup, when it shows.
    pub popup: Option<Popup>,
    /// The places that a click hits. The first place that holds a
    /// point wins.
    pub hits: Vec<Hit>,
    /// The place of the cursor of the terminal: the prompt of the Chat
    /// tab. `None` hides the cursor.
    pub cursor: Option<Position>,
    /// The window of the log of the Chat tab, while it shows
    /// ([`State::drawn`](super::State::drawn)).
    pub chat: Option<chat::Window>,
}

impl View {
    /// The key of the first place that holds the point at `column` and
    /// `row`.
    pub fn hit(&self, column: u16, row: u16) -> Option<Key> {
        let at = Position::new(column, row);
        self.hits
            .iter()
            .find(|hit| hit.area.contains(at))
            .map(|hit| hit.key)
    }
}

/// The style of a good word: `running`, `link ok`.
pub(super) const GOOD: Style = Style::new().fg(Color::Green);

/// The style of a word that needs a look: `paused`, another build.
pub(super) const WARNING: Style = Style::new().fg(Color::Yellow);

/// The style of a fault: `link down`, the fault line.
pub(super) const ERROR: Style = Style::new().fg(Color::Red);

/// The style of the key line.
pub(super) const DIM: Style = Style::new().add_modifier(Modifier::DIM);

/// The style of the active tab.
pub(super) const ACTIVE: Style = Style::new().add_modifier(Modifier::BOLD);

/// The style of the news of a tab.
const NEWS: Style = Style::new().fg(Color::Cyan);

/// The width of `text` in columns.
pub(super) fn width(text: &str) -> u16 {
    u16::try_from(Line::raw(text).width()).unwrap_or(u16::MAX)
}

/// The view of `state` in `area` at the time `now_ms`, in milliseconds
/// since the epoch (01M6VAZKNXZV1RTG7MT0GMHKT3). It reads only its
/// arguments.
pub fn compute_view(state: &State, area: Rect, now_ms: i64) -> View {
    let layout = Layout::of(area);
    if layout == Layout::Small {
        let rows = textwrap::wrap(TOO_SMALL, usize::from(area.width.max(1)))
            .into_iter()
            .zip(area.rows())
            .map(|(line, row)| Placed {
                area: row,
                line: Line::raw(line.into_owned()),
            })
            .collect();
        return View {
            rows,
            ..View::default()
        };
    }
    let margin = u16::from(layout == Layout::Wide);
    let row = |y: u16| Rect::new(area.x + margin, area.y + y, area.width - margin, 1);
    let mut view = View::default();

    let (strip, hits) = match layout {
        Layout::Wide => strip_wide(state, area),
        _ => strip_narrow(state, area),
    };
    view.rows.push(Placed {
        area: Rect::new(area.x, area.y, area.width, 1),
        line: strip,
    });
    view.hits.extend(hits);
    view.rows.push(Placed {
        area: row(1),
        line: header(state, layout, now_ms),
    });

    let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
    match state.tab() {
        Tab::Machines => {
            let (rows, hits) = super::machines::body(state, body, layout, now_ms);
            view.rows.extend(rows);
            view.hits.extend(hits);
        }
        Tab::Waves => {
            let rows = state.wave_rows();
            let lines = super::waves::body(
                state.waves(),
                &rows,
                layout,
                area.width - margin,
                body.height,
                now_ms,
            );
            for ((line, index), y) in lines.into_iter().zip(2..area.height - 2) {
                if rows[index].pick().is_some() {
                    view.hits.push(Hit {
                        area: row(y),
                        key: Key::Row(index),
                    });
                }
                view.rows.push(Placed { area: row(y), line });
            }
        }
        Tab::Chat => chat::place(state, area, layout, &mut view),
        tab => {
            for (line, y) in body_lines(tab, area.width - margin)
                .into_iter()
                .zip(2..area.height - 2)
            {
                view.rows.push(Placed { area: row(y), line });
            }
        }
    }

    if let Some(line) = note_line(state, now_ms) {
        view.rows.push(Placed {
            area: row(area.height - 2),
            line,
        });
    }
    let keys = row(area.height - 1);
    let (line, hits) = key_line(keys_of(state, layout), keys);
    view.rows.push(Placed { area: keys, line });
    view.hits.extend(hits);

    if state.help_shown() {
        view.popup = Some(popup(state.tab(), area, layout));
        view.cursor = None;
        // A click closes the popup, wherever it hits.
        view.hits = vec![Hit {
            area,
            key: Key::Esc,
        }];
    }
    view
}

/// The note line (01M6VMACJGF4A0ZXC6YK13KNCH): a question of the TUI
/// (01M6VFD0S90SB8M8WV952NT4Y1), else the fault of the link, else in
/// the Chat tab a post that failed after the last key or whom the line
/// in the prompt will wake ([`chat::note`]), else the note of the last
/// action, else in the Waves tab the age of its last read of `gh`
/// (01M6VFGYW0XVJ5JSX6R1NCR6W7).
fn note_line(state: &State, now_ms: i64) -> Option<Line<'static>> {
    if let Some(asked) = state.asked() {
        return Some(Line::styled(text::safe(&asked), ACTIVE));
    }
    if let Some(fault) = state.fault() {
        return Some(Line::styled(text::safe(fault), ERROR));
    }
    if state.tab() == Tab::Chat
        && let Some(line) = chat::note(state)
    {
        return Some(line);
    }
    if let Some(note) = state.shown_note() {
        return Some(Line::raw(text::safe(note)));
    }
    (state.tab() == Tab::Waves)
        .then(|| Line::styled(super::waves::note(state.waves(), now_ms), DIM))
}

/// The strip of the wide layout, and the place of each tab
/// (01M6VAZK1R932VQMJBC32MTTJB): the tabs with two spaces between
/// them, the active tab in `[ ]`, and the news of a tab after its name.
/// The digit of the first tab stays in the second column.
fn strip_wide(state: &State, area: Rect) -> (Line<'static>, Vec<Hit>) {
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut x = area.x;
    if state.tab() != Tab::Sessions {
        spans.push(Span::raw(" "));
        x += 1;
    }
    for tab in Tab::ALL {
        if tab != Tab::Sessions {
            spans.push(Span::raw("  "));
            x += 2;
        }
        let label = format!("{} {}", tab.number(), tab.name());
        let item = match (tab == state.tab(), state.news_of(tab)) {
            (true, _) => vec![Span::styled(format!("[{label}]"), ACTIVE)],
            (false, Some(news)) => vec![Span::raw(label), Span::styled(news.text(), NEWS)],
            (false, None) => vec![Span::raw(label)],
        };
        let wide = item.iter().map(|span| width(&span.content)).sum();
        hits.push(Hit {
            area: Rect::new(x, area.y, wide, 1).intersection(area),
            key: Key::Open(tab),
        });
        spans.extend(item);
        x += wide;
    }
    (Line::from(spans), hits)
}

/// The strip of the narrow layout: `‹ N/6 NAME ›`, with no news. A
/// click on `‹` or `›` opens the tab before or after.
fn strip_narrow(state: &State, area: Rect) -> (Line<'static>, Vec<Hit>) {
    let tab = state.tab();
    let middle = format!(" {}/{} {} ", tab.number(), Tab::ALL.len(), tab.name());
    let after = area.x + 1 + width(&middle);
    let hits = vec![
        Hit {
            area: Rect::new(area.x, area.y, 1, 1),
            key: Key::Before,
        },
        Hit {
            area: Rect::new(after, area.y, 1, 1).intersection(area),
            key: Key::Next,
        },
    ];
    let line = Line::from(vec![
        Span::raw("‹"),
        Span::styled(middle, ACTIVE),
        Span::raw("›"),
    ]);
    (line, hits)
}

/// The header (01M6VAZK4RQSZ0MG2DHWQC598Q): the state of the whole
/// riff, the owner, the build of this riff and of riff-server when it
/// is another, and the link. The narrow layout has only the state of
/// the riff and the link.
fn header(state: &State, layout: Layout, now_ms: i64) -> Line<'static> {
    let wide = layout == Layout::Wide;
    let mut parts: Vec<Vec<Span<'static>>> = Vec::new();
    let look = state.last_look();
    if let Some((look, _)) = look {
        let riff = if look.pauses.riff.is_some() {
            Span::styled("paused", WARNING)
        } else {
            Span::styled("running", GOOD)
        };
        parts.push(vec![Span::raw("riff "), riff]);
        if wide {
            match &look.who.owner {
                RiffOwner::NoSignIn => {}
                RiffOwner::Nobody => parts.push(vec![Span::raw("owner none")]),
                RiffOwner::Owner { user, .. } => {
                    parts.push(vec![Span::raw(format!("owner {}", text::safe(user)))]);
                }
            }
        }
    }
    if wide {
        let build = state.build();
        parts.push(vec![Span::raw(format!("build {}", short(&build.id)))]);
        if let Some(server) = look
            .and_then(|(look, _)| look.server.as_ref())
            .filter(|server| !server.matches(build))
        {
            parts.push(vec![Span::styled(
                format!("server {}", short(&server.id)),
                WARNING,
            )]);
        }
    }
    let link = match look {
        Some((_, at_ms)) if !state.link_down() => {
            let secs = u64::try_from((now_ms - at_ms) / 1000).unwrap_or(0);
            Span::styled(format!("ok {}", text::ago(secs)), GOOD)
        }
        _ => Span::styled("down", ERROR),
    };
    parts.push(vec![Span::raw("link "), link]);
    let mut spans = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.extend(part);
    }
    Line::from(spans)
}

/// The first 7 characters of a build ID.
pub(super) fn short(id: &str) -> String {
    text::safe(&id.chars().take(7).collect::<String>())
}

/// The body of `tab` in `columns`, until its build item merges
/// (01M6VAZKVK7530JJ63S7RVF88J): the commands that show its area.
fn body_lines(tab: Tab, columns: u16) -> Vec<Line<'static>> {
    let today = tab.today();
    let head = if today.is_empty() {
        format!("The {} tab is not built yet.", tab.name())
    } else {
        format!(
            "The {} tab is not built yet. Use these commands:",
            tab.name()
        )
    };
    let mut lines: Vec<Line<'static>> = textwrap::wrap(&head, usize::from(columns.max(1)))
        .into_iter()
        .map(|line| Line::raw(line.into_owned()))
        .collect();
    lines.extend(
        today
            .iter()
            .map(|command| Line::styled(format!("  {command}"), ACTIVE)),
    );
    lines
}

/// The keys of the key line of the tab of `state` in a layout: the text
/// of the key, its word, and the key that a click on it presses. A built
/// tab has its own keys first ([`super::machines::keys`]), or all its
/// keys ([`super::waves::keys_of`], [`chat::keys`]).
fn keys_of(state: &State, layout: Layout) -> Vec<(&'static str, &'static str, Option<Key>)> {
    let mut keys = match state.tab() {
        Tab::Chat => return chat::keys(state.chat().focus(), layout),
        Tab::Waves => return super::waves::keys_of(layout),
        Tab::Machines => super::machines::keys(layout),
        _ => vec![("1-6", "tab", None)],
    };
    if layout == Layout::Wide {
        keys.push(("r", "read again", Some(Key::Again)));
    }
    keys.extend([
        ("?", "help", Some(Key::Help)),
        ("q", "quit", Some(Key::Quit)),
    ]);
    keys
}

/// The key line of `keys` at `area`, and the place of each key that a
/// click presses (01M6VAZKDEQ6951KGPGAMF1SNJ).
fn key_line(
    keys: Vec<(&'static str, &'static str, Option<Key>)>,
    area: Rect,
) -> (Line<'static>, Vec<Hit>) {
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut x = area.x;
    for (i, (key, word, press)) in keys.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", DIM));
            x += 2;
        }
        let shown = format!("{key} {word}");
        let wide = width(&shown);
        if let Some(press) = press {
            hits.push(Hit {
                area: Rect::new(x, area.y, wide, 1).intersection(area),
                key: press,
            });
        }
        spans.push(Span::styled(shown, DIM));
        x += wide;
    }
    (Line::from(spans), hits)
}

/// The keys of the help popup of `tab`: the key, and what it does
/// (01M6VAZKDEQ6951KGPGAMF1SNJ). A built tab has its own keys first
/// ([`super::machines::help_keys`], [`super::waves::help_keys`],
/// [`chat::HELP`]). Each tab has the keys of the frame.
///
/// ```
/// use riff::tui::Tab;
/// use riff::tui::view::help_keys;
///
/// assert_eq!(help_keys(Tab::Sessions)[0], ("1 to 6", "open a tab"));
/// assert_eq!(help_keys(Tab::Waves)[0], ("↑ ↓, j k", "select a row"));
/// assert_eq!(help_keys(Tab::Machines)[0], ("↑ ↓ j k", "select a host"));
/// assert!(help_keys(Tab::Waves).contains(&("q, Ctrl-C", "end the TUI")));
/// assert!(help_keys(Tab::Machines).contains(&("q, Ctrl-C", "end the TUI")));
/// assert_eq!(help_keys(Tab::Chat)[7], ("/me TEXT", "send an action"));
/// assert!(help_keys(Tab::Chat).contains(&("q, Ctrl-C", "end the TUI")));
/// ```
pub fn help_keys(tab: Tab) -> Vec<(&'static str, &'static str)> {
    let mut keys = match tab {
        Tab::Waves => super::waves::help_keys(),
        Tab::Machines => super::machines::help_keys(),
        Tab::Chat => chat::HELP.to_vec(),
        _ => Vec::new(),
    };
    keys.extend([
        ("1 to 6", "open a tab"),
        ("Tab", "open the next tab"),
        ("Shift-Tab", "open the tab before"),
        ("r", "look again now"),
        ("?, Esc", "close this help"),
        ("q, Ctrl-C", "end the TUI"),
    ]);
    keys
}

/// The help popup of `tab` in the middle of `area`, under the strip and
/// over the key line.
fn popup(tab: Tab, area: Rect, layout: Layout) -> Popup {
    let keys = help_keys(tab);
    let column = keys.iter().map(|(key, _)| width(key)).max().unwrap_or(0) + 2;
    let mut lines: Vec<Line<'static>> = keys
        .iter()
        .map(|(key, does)| {
            Line::from(vec![
                Span::styled(format!(" {key:<pad$}", pad = usize::from(column)), ACTIVE),
                Span::raw(*does),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::raw(" The sessions go on after the TUI."));
    let title = format!(" Keys of the {} tab ", tab.name());
    let inner = lines
        .iter()
        .map(|line| u16::try_from(line.width()).unwrap_or(u16::MAX))
        .chain([width(&title)])
        .max()
        .unwrap_or(0)
        + 2;
    let room = match layout {
        Layout::Wide => area.width.saturating_sub(4),
        _ => area.width,
    };
    let wide = (inner + 2).min(room);
    let rows = area.height - 2;
    let high = (u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2).min(rows);
    Popup {
        area: Rect::new(
            area.x + (area.width - wide) / 2,
            area.y + 1 + (rows - high) / 2,
            wide,
            high,
        ),
        title,
        lines,
    }
}

/// Draws `view` on `buffer` (01M6VAZKNXZV1RTG7MT0GMHKT3). It reads only
/// the view.
pub fn render(view: &View, buffer: &mut Buffer) {
    for placed in &view.rows {
        (&placed.line).render(placed.area.intersection(buffer.area), buffer);
    }
    if let Some(popup) = &view.popup {
        let area = popup.area.intersection(buffer.area);
        Clear.render(area, buffer);
        let block = Block::bordered().title(popup.title.as_str());
        let inner = block.inner(area);
        block.render(area, buffer);
        for (line, row) in popup.lines.iter().zip(inner.rows()) {
            line.render(row, buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{Action, Look, News};
    use riff_core::build::Build;
    use riff_core::wire::RiffState;

    /// The time of the screen tests.
    const NOW: i64 = 1_000_000;

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// The state of the mock-ups of #994: a riff that runs, of the
    /// owner mike, at the same build as riff-server, 2 seconds after a
    /// good look, with two new lines in the chat and news in the waves.
    fn mock_up() -> State {
        let mut state = State::new(build("10df8a4c0ffe"));
        let mut look = Look::default();
        look.who.owner = RiffOwner::Owner {
            user: "mike".into(),
            email: "m@x.io".into(),
        };
        look.server = Some(build("10df8a4c0ffe"));
        state.looked(look, NOW - 2_000);
        state.news(Tab::Chat, News::Lines(2));
        state.news(Tab::Waves, News::Dot);
        state
    }

    /// The rows of `state` drawn into a `Buffer` of `width` x `height`,
    /// with no spaces at their ends.
    fn drawn(state: &State, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        render(&compute_view(state, area, NOW), &mut buffer);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// The frame of the mock-up of the Sessions tab of #994 at 80 x 24:
    /// the strip, the header, the note and the key line of the frame,
    /// and the body until item B (01M6VAZK1R932VQMJBC32MTTJB,
    /// 01M6VAZK4RQSZ0MG2DHWQC598Q, 01M6VAZKNXZV1RTG7MT0GMHKT3).
    #[test]
    fn the_frame_at_80_x_24_is_the_mock_up() {
        let mut state = mock_up();
        state.note("The lead of riff runs on. (Ctrl-P Ctrl-Q detached.)");
        let mut want = vec![
            "[1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports",
            " riff running · owner mike · build 10df8a4 · link ok 2s",
            " The Sessions tab is not built yet. Use these commands:",
            "   riff top",
            "   riff who",
        ];
        want.resize(22, "");
        want.extend([
            " The lead of riff runs on. (Ctrl-P Ctrl-Q detached.)",
            " 1-6 tab  r read again  ? help  q quit",
        ]);
        assert_eq!(drawn(&state, 80, 24), want);
    }

    /// The frame of the mock-up of the narrow layout of #994 at 40 x 24
    /// (01M6VAZKK4KHCDCX4RKG9B95D7).
    #[test]
    fn the_frame_at_40_x_24_is_the_narrow_mock_up() {
        let mut state = mock_up();
        state.note("The lead of riff runs on.");
        let mut want = vec![
            "‹ 1/6 Sessions ›",
            "riff running · link ok 2s",
            "The Sessions tab is not built yet. Use",
            "these commands:",
            "  riff top",
            "  riff who",
        ];
        want.resize(22, "");
        want.extend(["The lead of riff runs on.", "1-6 tab  ? help  q quit"]);
        assert_eq!(drawn(&state, 40, 24), want);
    }

    /// The strip of the mock-up of each tab of #994: the active tab in
    /// `[ ]`, and no news on it (01M6VAZK1R932VQMJBC32MTTJB).
    #[test]
    fn the_strip_of_each_tab_is_the_strip_of_its_mock_up() {
        let strips = [
            "[1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports",
            " 1 Sessions  [2 Chat]  3 Waves•  4 Machines  5 Devices  6 Reports",
            " 1 Sessions  2 Chat•2  [3 Waves]  4 Machines  5 Devices  6 Reports",
            " 1 Sessions  2 Chat•2  3 Waves•  [4 Machines]  5 Devices  6 Reports",
            " 1 Sessions  2 Chat•2  3 Waves•  4 Machines  [5 Devices]  6 Reports",
            " 1 Sessions  2 Chat•2  3 Waves•  4 Machines  5 Devices  [6 Reports]",
        ];
        for (tab, strip) in Tab::ALL.into_iter().zip(strips) {
            let mut state = mock_up();
            state.on_key(Key::Open(tab));
            let rows = drawn(&state, 80, 24);
            assert_eq!(rows[0], strip, "{tab:?}");
            // A built tab draws its own body.
            if tab == Tab::Chat {
                assert_eq!(rows[2], " chat · a line wakes your leads", "{rows:#?}");
            }
            if tab.built() {
                continue;
            }
            assert!(
                rows[2].starts_with(&format!(" The {} tab is not built yet.", tab.name())),
                "{rows:#?}"
            );
            for (y, command) in tab.today().iter().enumerate() {
                assert_eq!(rows[3 + y], format!("   {command}"), "{rows:#?}");
            }
        }
        // The narrow strip names the tab and its number.
        let mut state = mock_up();
        state.on_key(Key::Open(Tab::Reports));
        assert_eq!(drawn(&state, 40, 24)[0], "‹ 6/6 Reports ›");
    }

    /// 01M6VAZKAH6B53YKR8W4YKNSH6: a look that fails keeps the last
    /// screen, puts `link down` in the header, and the fault in the note
    /// line, until the next good look.
    #[test]
    fn a_look_that_fails_puts_link_down_in_the_header_and_the_fault_in_the_note() {
        let mut state = mock_up();
        state.note("an old note");
        let fault = "riff: no good look since 21:35:07: cannot reach riff-server";
        state.failed(fault);
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[1],
            " riff running · owner mike · build 10df8a4 · link down"
        );
        assert_eq!(rows[22], format!(" {fault}"));
        let narrow = drawn(&state, 40, 24);
        assert_eq!(narrow[1], "riff running · link down");
        assert_eq!(narrow[22], &fault[..40]);
        // The next good look: the fault goes, the note comes back.
        let (look, _) = state.last_look().unwrap();
        state.looked(look.clone(), NOW);
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[1],
            " riff running · owner mike · build 10df8a4 · link ok 0s"
        );
        assert_eq!(rows[22], " an old note");
    }

    /// 01M6VAZK4RQSZ0MG2DHWQC598Q: the header says when the whole riff
    /// is paused, when it has no owner, and the build of riff-server
    /// when it is another.
    #[test]
    fn the_header_shows_a_pause_no_owner_and_another_build_of_riff_server() {
        let mut state = State::new(build("10df8a4c0ffe"));
        let mut look = Look {
            pauses: RiffState::Paused.into(),
            ..Look::default()
        };
        look.who.owner = RiffOwner::Nobody;
        look.server = Some(build("3f2a91c00000"));
        state.looked(look, NOW - 125_000);
        assert_eq!(
            drawn(&state, 80, 24)[1],
            " riff paused · owner none · build 10df8a4 · server 3f2a91c · link ok 2m"
        );
        assert_eq!(drawn(&state, 40, 24)[1], "riff paused · link ok 2m");
        // No look yet: the link is down.
        let state = State::new(build("10df8a4c0ffe"));
        assert_eq!(drawn(&state, 80, 24)[1], " build 10df8a4 · link down");
    }

    /// 01M6VAZKDEQ6951KGPGAMF1SNJ: `?` shows the keys of the tab in a
    /// popup, and a click closes it (01M6VAZKG8FS4HEA9VSA4Z4YF1).
    #[test]
    fn the_help_popup_shows_the_keys_of_the_tab() {
        let mut state = mock_up();
        state.on_key(Key::Open(Tab::Waves));
        state.on_key(Key::Help);
        for (width, height) in [(80, 24), (40, 24), (40, 10)] {
            let rows = drawn(&state, width, height);
            let text = rows.join("\n");
            assert!(text.contains("Keys of the Waves tab"), "{text}");
            for (key, does) in help_keys(Tab::Waves) {
                if height > 10 {
                    assert!(
                        rows.iter().any(|r| r.contains(key) && r.contains(does)),
                        "{key}: {text}"
                    );
                }
            }
            // The strip stays above the popup.
            assert!(rows[0].contains("Waves"), "{text}");
        }
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(&state, area, NOW);
        let popup = view.popup.as_ref().unwrap();
        assert!(popup.area.width <= 76 && popup.area.height <= 24);
        // A click anywhere closes the popup.
        assert_eq!(state.on_click(&view, 79, 0), None);
        assert!(!state.help_shown());
        assert_eq!(compute_view(&state, area, NOW).popup, None);
        // `?` again shows it, and `?` closes it.
        state.on_key(Key::Help);
        state.on_key(Key::Help);
        assert!(!state.help_shown());
    }

    /// 01M6VAZKG8FS4HEA9VSA4Z4YF1: a click acts as the key of the place
    /// that it hits: a tab of the strip, `‹` and `›` of the narrow strip,
    /// and the keys of the key line.
    #[test]
    fn a_click_presses_the_key_of_its_place() {
        let mut state = mock_up();
        let wide = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        // The `3` of `3 Waves•` is at column 24 of the strip.
        let waves = "[1 Sessions]  2 Chat•2  ".chars().count();
        assert_eq!(
            wide.hit(u16::try_from(waves).unwrap(), 0),
            Some(Key::Open(Tab::Waves))
        );
        // The Waves tab looks again at once, for the holds of its boards.
        assert_eq!(state.on_click(&wide, 30, 0), Some(Action::Look));
        assert_eq!(state.tab(), Tab::Waves);
        assert_eq!(state.news_of(Tab::Waves), None);
        // The space between two tabs is no place.
        assert_eq!(wide.hit(12, 0), None);
        assert_eq!(wide.hit(0, 0), Some(Key::Open(Tab::Sessions)));
        // The key line: ` 1-6 tab  r read again  ? help  q quit`.
        let keys = " 1-6 tab  r read again  ? help  q quit";
        let at = |word: &str| u16::try_from(keys.find(word).unwrap()).unwrap();
        assert_eq!(wide.hit(at("1-6"), 23), None);
        assert_eq!(wide.hit(at("read again"), 23), Some(Key::Again));
        assert_eq!(wide.hit(at("? help"), 23), Some(Key::Help));
        assert_eq!(state.on_click(&wide, at("q quit"), 23), Some(Action::Quit));
        // No place in the body.
        assert_eq!(wide.hit(5, 5), None);

        let narrow = compute_view(&state, Rect::new(0, 0, 40, 24), NOW);
        let next = u16::try_from("‹ 3/6 Waves ".chars().count()).unwrap();
        assert_eq!(narrow.hit(next, 0), Some(Key::Next));
        assert_eq!(narrow.hit(0, 0), Some(Key::Before));
        assert_eq!(narrow.hit(3, 0), None);
        state.on_click(&narrow, 0, 0);
        assert_eq!(state.tab(), Tab::Chat);
        let narrow = compute_view(&state, Rect::new(0, 0, 40, 24), NOW);
        let next = u16::try_from("‹ 2/6 Chat ".chars().count()).unwrap();
        state.on_click(&narrow, next, 0);
        assert_eq!(state.tab(), Tab::Waves);
    }

    /// 01M6VAZKK4KHCDCX4RKG9B95D7: under 40 columns or 10 rows, the TUI
    /// asks for a bigger terminal.
    #[test]
    fn a_terminal_that_is_too_small_gets_a_line_that_asks_for_more() {
        let state = mock_up();
        for (width, height) in [(39, 24), (80, 9), (20, 4)] {
            let text = drawn(&state, width, height).join(" ");
            let words: Vec<&str> = text.split_whitespace().collect();
            let want: Vec<&str> = TOO_SMALL.split_whitespace().collect();
            assert!(want.starts_with(&words), "{width} x {height}: {text}");
            assert!(!text.contains("Sessions"), "{text}");
            let view = compute_view(&state, Rect::new(0, 0, width, height), NOW);
            assert!(view.hits.is_empty());
        }
        let text = drawn(&state, 39, 24).join(" ");
        let words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(words, TOO_SMALL.split_whitespace().collect::<Vec<_>>());
    }

    /// A wide terminal draws the same frame as 80 columns.
    #[test]
    fn a_wide_terminal_draws_the_same_frame() {
        let state = mock_up();
        let wide = drawn(&state, 200, 30);
        let narrow = drawn(&state, 80, 30);
        assert_eq!(wide, narrow);
        assert_eq!(wide[29], " 1-6 tab  r read again  ? help  q quit");
    }

    /// A view starts at the corner of its area: a frame of an area that
    /// does not start at 0 draws at its place.
    #[test]
    fn the_view_draws_in_its_area() {
        let state = mock_up();
        let area = Rect::new(3, 2, 80, 24);
        let view = compute_view(&state, area, NOW);
        assert!(
            view.rows
                .iter()
                .all(|placed| area.contains(placed.area.as_position()))
        );
        assert_eq!(view.hit(3, 2), Some(Key::Open(Tab::Sessions)));
        assert_eq!(view.hit(2, 2), None);
    }
}
