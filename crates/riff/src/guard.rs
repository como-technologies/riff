//! The guard of riff on GitHub: the workflow [`PATH`] in each
//! repository of the riff (01M6WPV2HK96JH900RX9ZPYHJ3).
//!
//! # Design
//!
//! A worker and a verifier act on GitHub as the bot of the session App
//! ([`crate::github`]). The right `issues: write` has no smaller part, so
//! their token can change the body or the title of an issue, and change
//! or delete a comment of another account. The guard puts back the old
//! text with `GITHUB_TOKEN`:
//!
//! ```mermaid
//! flowchart LR
//!     E["issues: edited<br/>issue_comment: edited, deleted"] --> Q{"the sender is<br/>the bot of the session App?"}
//!     Q -- "no: a person, the lead App" --> K["keep the change"]
//!     Q -- yes --> R["put back the old text<br/>with GITHUB_TOKEN"] --> C["comment why (#1537)"]
//! ```
//!
//! | Event | A change of the bot | The guard |
//! |---|---|---|
//! | `issues: edited` | the title or the body of an issue | puts back each from `changes.title.from` and `changes.body.from`, and comments why |
//! | `issue_comment: edited` | a comment of another account | puts back the old text from `changes.body.from` |
//! | `issue_comment: deleted` | a comment of another account | posts the text again, after one line that names the author and the time |
//!
//! - The `if` of the job compares the sender with the bot ([`condition`]),
//!   so a change of a person, of the lead App or of another bot starts no
//!   runner. A change of the bot to its own comment stays.
//! - A change with `GITHUB_TOKEN` starts no workflow: the guard does not
//!   run on its own change.
//! - The script ([`SCRIPT`]) uses only `bash` and `gh`. It reads each fact
//!   of the event from the env of its step ([`ENV`]), so no text of the
//!   event is a part of the script (01M6WPV81PJDTFK6DAJ6MJHRQ3).
//! - The guard does not stop the change: the issue shows it for some
//!   seconds, and the history of the issue keeps both edits.
//!
//! `riff init` writes the guard with the bot of the session App of the
//! riff ([`workflow`], [`session_bot`]), in the pull request "Set up riff"
//! ([`crate::repo_setup`]).
//!
//! ```
//! let text = riff::guard::workflow("riff-acme[bot]");
//! assert!(text.contains("if: github.event.sender.login == 'riff-acme[bot]'\n"));
//! assert!(text.contains("          OLD_BODY: ${{ github.event.changes.body.from }}\n"));
//! ```

use riff_client::api::Api;
use riff_core::wire::GitHubApp;

use crate::text;

/// The path of the guard in a repository.
pub const PATH: &str = ".github/workflows/riff-guard.yml";

/// The name of the workflow file of the guard, for
/// `gh run list --workflow`.
pub const FILE: &str = "riff-guard.yml";

/// The env of the step of the guard: each name, with the expression of
/// GitHub that gives its value.
pub const ENV: [(&str, &str); 14] = [
    ("GH_TOKEN", "github.token"),
    ("GH_REPO", "github.repository"),
    ("EVENT", "github.event_name"),
    ("ACTION", "github.event.action"),
    ("SENDER", "github.event.sender.login"),
    ("NUMBER", "github.event.issue.number"),
    ("TITLE_CHANGED", "github.event.changes.title != null"),
    ("OLD_TITLE", "github.event.changes.title.from"),
    ("BODY_CHANGED", "github.event.changes.body != null"),
    ("OLD_BODY", "github.event.changes.body.from"),
    ("COMMENT", "github.event.comment.id"),
    ("AUTHOR", "github.event.comment.user.login"),
    ("TEXT", "github.event.comment.body"),
    ("WRITTEN", "github.event.comment.created_at"),
];

/// The script of the step of the guard, for `bash -eo pipefail`. Its job
/// runs only for the bot of the session App ([`condition`]).
pub const SCRIPT: &str = r#"call() { gh api "$@" > /dev/null; }
comment() { call --method POST "repos/$GH_REPO/issues/$NUMBER/comments" -f "body=$1"; }
case "$EVENT.$ACTION" in
  issues.edited)
    fields=()
    words=""
    if [ "$TITLE_CHANGED" = true ]; then
      fields+=(-f "title=$OLD_TITLE")
      words="the title"
    fi
    if [ "$BODY_CHANGED" = true ]; then
      fields+=(-f "body=$OLD_BODY")
      words="${words:+$words and }the body"
    fi
    [ -n "$words" ] || exit 0
    call --method PATCH "repos/$GH_REPO/issues/$NUMBER" "${fields[@]}"
    comment "riff put back $words of this issue: a worker or a verifier changed it. A session adds to an issue with a comment (como-technologies/riff#1537)."
    ;;
  issue_comment.edited)
    # A change of the bot to its own comment stays.
    [ "$AUTHOR" != "$SENDER" ] && [ "$BODY_CHANGED" = true ] || exit 0
    call --method PATCH "repos/$GH_REPO/issues/comments/$COMMENT" -f "body=$OLD_BODY"
    ;;
  issue_comment.deleted)
    [ "$AUTHOR" != "$SENDER" ] || exit 0
    comment "riff put back this comment: a worker or a verifier deleted it. $AUTHOR wrote it at $WRITTEN.

$TEXT"
    ;;
esac
"#;

/// The condition of the job of the guard: the sender of the event is
/// `bot`.
///
/// ```
/// assert_eq!(
///     riff::guard::condition("riff-acme[bot]"),
///     "github.event.sender.login == 'riff-acme[bot]'"
/// );
/// ```
pub fn condition(bot: &str) -> String {
    format!("github.event.sender.login == '{bot}'")
}

/// True when `bot` is the login of the bot of a GitHub App: `SLUG[bot]`,
/// with a slug of letters, digits and `-`. Only such a login goes into
/// the guard.
///
/// ```
/// use riff::guard::valid_bot;
///
/// assert!(valid_bot("riff-como-technologies[bot]"));
/// assert!(!valid_bot("riff-acme"));
/// assert!(!valid_bot("[bot]"));
/// assert!(!valid_bot("x' || true || '[bot]"));
/// ```
pub fn valid_bot(bot: &str) -> bool {
    bot.strip_suffix("[bot]").is_some_and(|slug| {
        !slug.is_empty() && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// The text of the guard for the bot `bot` of the session App
/// (01M6WPV2HK96JH900RX9ZPYHJ3). `riff init` writes it to [`PATH`]. Each
/// line of [`ENV`] and of [`SCRIPT`] is in it.
///
/// ```
/// use riff::guard::{ENV, SCRIPT, workflow};
///
/// let text = workflow("riff-acme[bot]");
/// for (name, expression) in ENV {
///     assert!(text.contains(&format!("\n          {name}: ${{{{ {expression} }}}}\n")), "{name}");
/// }
/// for line in SCRIPT.lines().filter(|l| !l.is_empty()) {
///     assert!(text.contains(&format!("\n          {line}\n")), "{line}");
/// }
/// ```
pub fn workflow(bot: &str) -> String {
    let env: String = ENV
        .iter()
        .map(|(name, expression)| format!("          {name}: ${{{{ {expression} }}}}\n"))
        .collect();
    let script: String = SCRIPT
        .lines()
        .map(|line| match line {
            "" => "\n".to_owned(),
            line => format!("          {line}\n"),
        })
        .collect();
    format!(
        "# The guard of riff. riff init writes this file with the bot of the\n\
         # session App of the riff. Each worker and each verifier of riff acts\n\
         # on GitHub as that bot. When the bot changes the body or the title of\n\
         # an issue, or changes or deletes a comment of another account, the\n\
         # guard puts back the old text with GITHUB_TOKEN. See \"Add to an\n\
         # issue\" in the book of riff:\n\
         # https://como-technologies.github.io/riff/how-it-works.html#add-to-an-issue\n\
         name: riff guard\n\
         \n\
         on:\n\
         \x20 issues:\n\
         \x20   types: [edited]\n\
         \x20 issue_comment:\n\
         \x20   types: [edited, deleted]\n\
         \n\
         permissions:\n\
         \x20 issues: write\n\
         \n\
         jobs:\n\
         \x20 guard:\n\
         \x20   name: Guard\n\
         \x20   # A change of a person, of the lead App or of another bot stays.\n\
         \x20   if: {condition}\n\
         \x20   runs-on: ubuntu-latest\n\
         \x20   timeout-minutes: 5\n\
         \x20   steps:\n\
         \x20     - name: Put back the change of the bot\n\
         \x20       shell: bash\n\
         \x20       env:\n\
         {env}\
         \x20       run: |\n\
         {script}",
        condition = condition(bot),
    )
}

/// The bot of the session App of the riff of `api`, as riff-server
/// gives it (01M6WPV8473EBABR1ZMEPZMTQQ). `Ok(None)` when the riff has no
/// session App: a riff with no sign-in has none. `Err` says why riff
/// cannot ask.
pub async fn session_bot(api: &Api) -> Result<Option<String>, String> {
    let apps = match api.github_apps(None, None).await {
        Ok(apps) => apps,
        // A riff with no sign-in has no session App.
        Err(_) if matches!(api.has_sign_in().await, Ok(false)) => return Ok(None),
        Err(e) => return Err(text::guard_unasked(api.base(), &format!("{e:#}"))),
    };
    match apps
        .apps
        .into_iter()
        .find(|app| app.app == GitHubApp::Session)
    {
        None => Ok(None),
        Some(app) if valid_bot(&app.bot) => Ok(Some(app.bot)),
        Some(app) => Err(text::guard_bad_bot(&app.bot)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workflow names the bot in its condition only, and the script
    /// reads each name of [`ENV`] that it uses.
    #[test]
    fn the_workflow_names_the_bot_once() {
        let text = workflow("riff-acme[bot]");
        assert_eq!(text.matches("riff-acme[bot]").count(), 1, "{text}");
        for name in [
            "GH_REPO", "EVENT", "ACTION", "SENDER", "NUMBER", "OLD_BODY", "AUTHOR", "TEXT",
        ] {
            assert!(SCRIPT.contains(&format!("${name}")), "{name}");
        }
        let names: Vec<&str> = ENV.iter().map(|(name, _)| *name).collect();
        for word in SCRIPT.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$') {
            if let Some(name) = word.strip_prefix('$')
                && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                && !name.is_empty()
            {
                assert!(
                    names.contains(&name),
                    "the script reads ${name}, which is no env"
                );
            }
        }
    }

    /// The events of the guard, and its one right.
    #[test]
    fn the_workflow_runs_on_the_edits_and_deletes_and_writes_issues_only() {
        let text = workflow("riff-acme[bot]");
        assert!(text.contains("  issues:\n    types: [edited]\n"), "{text}");
        assert!(
            text.contains("  issue_comment:\n    types: [edited, deleted]\n"),
            "{text}"
        );
        assert!(
            text.contains("permissions:\n  issues: write\n\njobs:"),
            "{text}"
        );
        // An empty line of the script stays empty in the YAML.
        assert!(!text.lines().any(|l| !l.is_empty() && l.trim().is_empty()));
    }
}
