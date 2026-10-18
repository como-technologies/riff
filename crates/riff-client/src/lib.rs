//! `riff-client`: the client of `riff-server`, the core library of each
//! UI of riff (01M54216KMTGS7KD1R3RY5JKQ0).
//!
//! # Design
//!
//! The book page "Design: the UIs of riff" (`docs/src/riff-uis.md`) has
//! the big picture. Each UI is a thin layer over this crate. Today the
//! one UI is the crate `riff`: the CLI, `riff mcp` and the hooks.
//!
//! ```mermaid
//! flowchart TB
//!     R["riff<br/>the CLI, riff mcp, the hooks,<br/>the riff daemon"] --> C
//!     R -- "its platform: the OS keyring,<br/>the local folders, the terminal" --> P
//!     C["riff-client<br/>the sign-in, the tokens,<br/>the API calls, the watch stream,<br/>the models of the views"] --> P["platform<br/>the parts that each UI gives"]
//!     C --> K["riff-core<br/>the model that the clients<br/>and riff-server share"]
//!     C -- "HTTPS" --> S["riff-server"]
//!     S --> K
//! ```
//!
//! # What the crate holds
//!
//! | Module | Holds |
//! |---|---|
//! | [`api`] | The calls of the HTTP API, and the tokens of a person and of a session. |
//! | [`link`] | The one link of each server: the budget of a call, the rule for a new try, the call ID, the reconnect. |
//! | [`login`] | The sign-in of the device: the token pair, its refresh and the token exchange of a session. |
//! | [`device`] | The device key of each server, and its DPoP proofs. |
//! | [`secrets`] | Each secret in the store of `keyring-core`. |
//! | [`marks`] | The marks of the sessions that left the riff. |
//! | [`files`] | The files in the local folders: mode 0700 for a folder, 0600 for a file. |
//! | [`text`] | The words of the errors and of the notes of the client. |
//! | [`platform`] | The trait of the parts that each UI gives. |
//! | [`sessions`] | The sessions model: the tree of `riff top` and the rows of `riff view`. |
//! | [`threads`] | The threads model: the lines of `riff read`, `riff tail` and `riff chat`. |
//! | [`waves`] | The waves model: the board of `riff top` and the waves of `riff wave list`. |
//! | [`machines`] | The machines model: the numbers of each host in `riff top` and `riff workers`. |
//! | [`daemons`] | The daemons model: the facts of the riff daemon of each host in `riff who` and `riff top`. |
//!
//! # What a UI gives
//!
//! A part that is not the same on each platform is a method of
//! [`platform::Platform`] (01M54216S51X5VBXN3YAERW6J6). A UI gives its
//! platform at its start with [`platform::set`]:
//!
//! - the store of the tokens and of the device key: a store of
//!   `keyring-core`, for example the OS keyring of a machine or the
//!   Keystore of a phone;
//! - the local folders: the folder of the refresh lock of the sign-in,
//!   and the folder of the leave marks;
//! - the name of the machine and the riff of the process;
//! - where a wait line and a note of another build go.
//!
//! The browser step of `riff login` (the local port of the OAuth
//! redirect) and the socket of the riff daemon stay in `riff`.
//!
//! # The models of the views
//!
//! Each model is plain data with serde (01M56Y8KAPHJMSK89ZW1PA8B0E).
//! The crate makes it from the replies of riff-server, and from what
//! the UI gives: the container sessions of its riff daemon, and the
//! reads of the forge. A UI only draws a model: the plain lines, the
//! colors and the widgets stay in the UI.
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server"] --> M["the models:<br/>sessions, threads,<br/>waves, machines"]
//!     U["the UI: the riff daemon,<br/>the forge, this machine"] --> M
//!     M --> D["the UI draws:<br/>plain lines, colors, widgets"]
//! ```
//!
//! # No terminal code
//!
//! The crate depends on no crate of the terminal: no ratatui, no
//! crossterm, no clap and no rustyline (01M54216PBE319RASGNMY9F0MR).
//! The test `riff_client_has_no_terminal_crate` of the crate `hygiene`
//! checks it. `riff-server` does not use this crate.

pub mod api;
pub mod daemons;
pub mod device;
pub mod files;
pub mod link;
pub mod login;
pub mod machines;
pub mod marks;
pub mod platform;
pub mod secrets;
pub mod sessions;
pub mod text;
pub mod threads;
pub mod waves;
