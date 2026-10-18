//! The words of `riff-client` for people and agents: its errors, and
//! the notes that its calls post.

use riff_core::name::ThreadName;
use riff_core::wire::{AdminSet, Invited, OwnerPassed, Removed, RiffState};

/// The end of the refusal of the `leave` tool when riff has no
/// directory for the mark of the leave (01M3XQVK05FAT3PR43W8RNEYHY).
pub const LEAVE_NO_MARK: &str = "riff has no directory for the mark of the leave: set HOME";

/// The refusal of a `riff` command that acts as a session that left the
/// riff, and of each request of such a session
/// (01M3MEEFETT9A0DRWBKQTG77Z2, 01M3XQVJXWBC3DKAVWBPXPSGZS).
pub const LEFT_COMMAND: &str = "this session left the riff. Run /riff:join in the session to \
join again";

/// The riff, or the repository `repository`: the subject of a pause.
pub fn pause_subject(repository: Option<&ThreadName>) -> String {
    match repository {
        Some(repository) => format!("The repository {repository}"),
        None => "The riff".to_owned(),
    }
}

/// The message that wakes the sessions after a pause or a resume
/// (01M3JCG3YD7C2Y3V0QJPF082YH): of the repository `repository`, or of
/// the whole riff.
///
/// ```
/// use riff_core::wire::RiffState;
///
/// assert!(riff_client::text::riff_news(None, RiffState::Paused).contains("\"Pause\""));
/// assert!(riff_client::text::riff_news(None, RiffState::Running).contains("from where you stopped"));
/// let strata = "como-technologies/strata".parse()?;
/// let news = riff_client::text::riff_news(Some(&strata), RiffState::Paused);
/// assert!(news.starts_with("The repository como-technologies/strata is paused. Stop"), "{news}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn riff_news(repository: Option<&ThreadName>, state: RiffState) -> String {
    let subject = pause_subject(repository);
    match state {
        RiffState::Paused => format!(
            "{subject} is paused. Stop at your next step and wait: see \"Pause\" in the riff \
             skill."
        ),
        RiffState::Running => format!(
            "{subject} is running again. Go on from where you stopped. A session with no work \
             follows the start routine."
        ),
    }
}

/// The error when the server is a new riff: the sign-in of this machine
/// is for a riff that is gone (01M3JNVBRS35B3CD67367JF7SJ). riff removed
/// it.
///
/// ```
/// assert_eq!(
///     riff_client::text::new_riff("http://h:7878"),
///     "This riff is new. Run riff login. \
///      (riff removed the old sign-in of this machine for http://h:7878.)"
/// );
/// ```
pub fn new_riff(server: &str) -> String {
    format!(
        "This riff is new. Run riff login. \
         (riff removed the old sign-in of this machine for {server}.)"
    )
}

/// The note of `riff invite` in each repository thread
/// (01M3MN14ZCTRVD3T455P6TFK1B). `user` made the change.
///
/// ```
/// use riff_core::wire::Invited;
///
/// let done = Invited { email: "bob@gmail.com".into(), address: "https://r.io".into() };
/// assert_eq!(
///     riff_client::text::invited_news("ada", &done),
///     "members: ada invited bob@gmail.com. bob@gmail.com is a member now."
/// );
/// ```
pub fn invited_news(user: &str, done: &Invited) -> String {
    let email = &done.email;
    format!("members: {user} invited {email}. {email} is a member now.")
}

/// The note of `riff remove` in each repository thread
/// (01M3MN14ZCTRVD3T455P6TFK1B).
///
/// ```
/// use riff_core::wire::Removed;
///
/// let done = Removed { email: "bob@gmail.com".into(), sign_ins: 2 };
/// assert_eq!(
///     riff_client::text::removed_news("ada", &done),
///     "members: ada removed bob@gmail.com. bob@gmail.com is not a member now."
/// );
/// ```
pub fn removed_news(user: &str, done: &Removed) -> String {
    let email = &done.email;
    format!("members: {user} removed {email}. {email} is not a member now.")
}

/// The note of `riff admin add` and `riff admin remove` in each
/// repository thread (01M3MN14ZCTRVD3T455P6TFK1B).
///
/// ```
/// use riff_core::wire::AdminSet;
///
/// let added = AdminSet { email: "bob@gmail.com".into(), admin: true };
/// assert_eq!(
///     riff_client::text::admin_news("ada", &added),
///     "members: ada made bob@gmail.com an admin."
/// );
/// let removed = AdminSet { email: "bob@gmail.com".into(), admin: false };
/// assert_eq!(
///     riff_client::text::admin_news("ada", &removed),
///     "members: ada made bob@gmail.com a member again, not an admin."
/// );
/// ```
pub fn admin_news(user: &str, done: &AdminSet) -> String {
    let email = &done.email;
    if done.admin {
        format!("members: {user} made {email} an admin.")
    } else {
        format!("members: {user} made {email} a member again, not an admin.")
    }
}

/// The note of `riff owner` in each repository thread
/// (01M3MN14ZCTRVD3T455P6TFK1B).
///
/// ```
/// use riff_core::wire::OwnerPassed;
///
/// let done = OwnerPassed { owner: "bob@gmail.com".into(), admin: "ada@gmail.com".into() };
/// assert_eq!(
///     riff_client::text::owner_news("ada", &done),
///     "members: ada passed the owner role to bob@gmail.com. ada@gmail.com stays an admin."
/// );
/// ```
pub fn owner_news(user: &str, done: &OwnerPassed) -> String {
    format!(
        "members: {user} passed the owner role to {}. {} stays an admin.",
        done.owner, done.admin
    )
}

/// The error at a riff with no sign-in, when a client of this machine
/// has an old sign-in for it (R226). `kept` is true while the keyring
/// still holds that sign-in. It is false when the sign-in was removed
/// after this process started.
///
/// ```
/// let server = "http://127.0.0.1:7878";
/// assert_eq!(
///     riff_client::text::no_sign_in(server, true),
///     "riff-server at http://127.0.0.1:7878 has no sign-in, but this machine \
///      has an old sign-in for it. Run riff logout, then try again."
/// );
/// assert_eq!(
///     riff_client::text::no_sign_in(server, false),
///     "riff-server at http://127.0.0.1:7878 has no sign-in. This process \
///      started with an old sign-in of this machine. Start your agent \
///      session again, or run the command again."
/// );
/// ```
pub fn no_sign_in(server: &str, kept: bool) -> String {
    if kept {
        format!(
            "riff-server at {server} has no sign-in, but this machine has an old \
             sign-in for it. Run riff logout, then try again."
        )
    } else {
        format!(
            "riff-server at {server} has no sign-in. This process started with an \
             old sign-in of this machine. Start your agent session again, or run the \
             command again."
        )
    }
}

/// The error of `riff logout --all` at a riff with no sign-in (R227).
///
/// ```
/// assert_eq!(
///     riff_client::text::nobody_signs_in("http://127.0.0.1:7878"),
///     "riff-server at http://127.0.0.1:7878 has no sign-in. Nobody is signed in to it."
/// );
/// ```
pub fn nobody_signs_in(server: &str) -> String {
    format!("riff-server at {server} has no sign-in. Nobody is signed in to it.")
}

/// The line of a locked OS keyring on `host`, or of one that does not
/// answer (01M4385CCATXC0B8HV1XD6EFWG). See [`crate::secrets::Locked`].
///
/// ```
/// assert_eq!(
///     riff_client::text::keyring_locked("pangolin"),
///     "the OS keyring of pangolin is locked or does not answer: unlock it at the desktop. \
///      gh uses the same keyring, so gh stops too"
/// );
/// ```
pub fn keyring_locked(host: &str) -> String {
    format!(
        "the OS keyring of {host} is locked or does not answer: unlock it at the desktop. \
         gh uses the same keyring, so gh stops too"
    )
}

/// The error of a call that got no reply from the server at `base` in
/// `wait` (01M3WN72M02P3J24ACCHTMNSFY).
///
/// ```
/// use std::time::Duration;
///
/// assert_eq!(
///     riff_client::text::no_reply("http://127.0.0.1:7878", Duration::from_secs(20)),
///     "riff-server at http://127.0.0.1:7878 gave no reply in 20 seconds"
/// );
/// assert_eq!(
///     riff_client::text::no_reply("http://127.0.0.1:7878", Duration::from_millis(10)),
///     "riff-server at http://127.0.0.1:7878 gave no reply in 0.01 seconds"
/// );
/// ```
pub fn no_reply(base: &str, wait: std::time::Duration) -> String {
    format!(
        "riff-server at {base} gave no reply in {} seconds",
        wait.as_secs_f64()
    )
}
