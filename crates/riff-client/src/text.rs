//! The words of `riff-client` for people and agents: its errors, and
//! the notes that its calls post.

use riff_core::name::ThreadName;
use riff_core::wire::{PauseInfo, RiffState};

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
/// let paused = riff_client::text::riff_news(None, RiffState::Paused);
/// assert!(paused.ends_with("see \"Pause\" in the instructions of the riff tools."), "{paused}");
/// assert!(!paused.contains("skill"));
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
            "{subject} is paused. Stop at your next step and wait: see \"Pause\" in the \
             instructions of the riff tools."
        ),
        RiffState::Running => format!("{subject} is running again. Go on from where you stopped."),
    }
}

/// The message to the lead of the repository `repository`, after a pause
/// or a resume of the whole riff, when the repository has the pause
/// `pause` of its own: the lead keeps the running state, and its workers
/// do not wake.
///
/// ```
/// use riff_core::record::By;
/// use riff_core::wire::{PauseInfo, RiffState};
///
/// let strata = "como-technologies/strata".parse()?;
/// let pause = PauseInfo { by: Some(By::Person("mike".into())), at_ms: 7 };
/// assert_eq!(
///     riff_client::text::riff_lead_news(&strata, RiffState::Running, &pause),
///     "The riff is running again. The repository como-technologies/strata is still paused by \
///      the person mike, so its workers did not wake. Your user or you resume it with `riff \
///      resume`."
/// );
/// assert_eq!(
///     riff_client::text::riff_lead_news(&strata, RiffState::Paused, &PauseInfo::default()),
///     "The riff is paused. The repository como-technologies/strata was paused already, so its \
///      workers did not wake. Your messages and tools still work."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn riff_lead_news(repository: &ThreadName, state: RiffState, pause: &PauseInfo) -> String {
    let by = pause
        .by
        .as_ref()
        .map_or_else(String::new, |by| format!(" by {by}"));
    match state {
        RiffState::Paused => format!(
            "The riff is paused. The repository {repository} was paused already{by}, so its \
             workers did not wake. Your messages and tools still work."
        ),
        RiffState::Running => format!(
            "The riff is running again. The repository {repository} is still paused{by}, so its \
             workers did not wake. Your user or you resume it with `riff resume`."
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
