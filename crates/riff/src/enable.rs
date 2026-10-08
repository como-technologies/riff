//! Whether riff is on in a process.
//!
//! # Design
//!
//! riff is on only in a session that riff started
//! (01M4BYH80CFW1TBGKVA2VN9ZBQ). riff gives each `claude` that it starts
//! the variable `RIFF_ON=1` ([`crate::launch::ON`]), and each process of
//! that session gets it from `claude`.
//!
//! ```mermaid
//! flowchart LR
//!     H["a hook, riff statusline,<br/>riff mcp"] --> E{"RIFF_ON=1?"}
//!     E -- yes --> ON[riff acts]
//!     E -- no --> OFF["riff does nothing"]
//! ```
//!
//! Each entry of the plugin asks [`on`] first: the hooks,
//! `riff statusline` and `riff mcp`. When riff is off, they call no
//! server, run no `git`, and print nothing
//! (01M3XY2ST8R67SKTXJECAYJZRX). So a plain `claude` with the plugin of
//! an older release does nothing of riff. `just dev` and the tests set
//! `RIFF_ON=1` too (01M3XY2SWEK0N8MC3MY4TMYTD3).
//!
//! ```
//! assert!(riff::enable::on_value(Some("1")));
//! assert!(!riff::enable::on_value(Some("0")));
//! assert!(!riff::enable::on_value(None));
//! ```

/// The variable that turns riff on for a process.
pub const VAR: &str = "RIFF_ON";

/// True when `RIFF_ON` is `1`.
pub fn on() -> bool {
    on_value(std::env::var(VAR).ok().as_deref())
}

/// [`on`] for the value of `RIFF_ON`.
pub fn on_value(value: Option<&str>) -> bool {
    value == Some("1")
}
