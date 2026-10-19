//! The checks of the notes of the lead (01M59258K3TYR09HC4AWHWZPEX).
//!
//! The notes of the lead are text that riff-server keeps for each user and
//! repository. The server and the client use the same checks:
//! [`check`] refuses a text over [`NOTES_MAX`] bytes
//! (01M59258K3TYR09HC4AWHWZPEX), and a text that looks like a token
//! (01M59258RJ3CG498Z8AVQ5TV5Q).
//!
//! ```
//! use riff_core::notes::{NOTES_MAX, check};
//!
//! assert!(check("Prefer thelio for builds. Commit 3f2a9c1d.").is_ok());
//! assert!(check(&"a ".repeat(NOTES_MAX)).unwrap_err().contains("64 KiB"));
//! assert!(check("the token is sk-ant-oat01-abcdefghijklmnop").unwrap_err().contains("secret"));
//! ```

/// The most bytes of the notes of one lead: 64 KiB.
pub const NOTES_MAX: usize = 64 * 1024;

/// The start of a word that is a token, with the least length of the
/// word.
const PREFIXES: [(&str, usize); 11] = [
    ("sk-ant-", 16),
    ("sk-", 24),
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("ghr_", 20),
    ("github_pat_", 24),
    ("glpat-", 20),
    ("xox", 20),
    ("AKIA", 20),
];

/// The least length of a run of letters and digits, with both, that
/// counts as a token.
const RUN: usize = 32;

/// Says if `text` can be the notes of a lead: `Err` has the reason.
///
/// ```
/// use riff_core::notes::check;
///
/// assert!(check("").is_ok());
/// // A branch name, a path and a requirement ID are no token.
/// assert!(check("branch claude-token-keyring-instead-of-a-file-1166").is_ok());
/// assert!(check("see crates/riff/src/lead.rs and 01M59258K3TYR09HC4AWHWZPEX").is_ok());
/// // A hash is hex.
/// assert!(check(&format!("sha {}", "ab12".repeat(16))).is_ok());
/// // A token.
/// assert!(check("GH_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789").is_err());
/// assert!(check("key abcdefghijklmnopqrstuvwxyz0123456789ABCD").is_err());
/// assert!(check("-----BEGIN OPENSSH PRIVATE KEY-----").is_err());
/// ```
pub fn check(text: &str) -> Result<(), String> {
    if text.len() > NOTES_MAX {
        return Err(format!(
            "the notes have {} bytes, and the limit is {NOTES_MAX} bytes (64 KiB). Make the text shorter.",
            text.len()
        ));
    }
    if looks_like_secret(text) {
        return Err(
            "the text looks like it has a secret (a token or a key). Take it out: riff never \
             keeps a secret in the notes."
                .to_owned(),
        );
    }
    Ok(())
}

fn looks_like_secret(text: &str) -> bool {
    if text.contains("PRIVATE KEY-----") {
        return true;
    }
    let words = text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    let prefixed = words.clone().any(|word| {
        PREFIXES
            .iter()
            .any(|(prefix, least)| word.starts_with(prefix) && word.len() >= *least)
    });
    prefixed
        || text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(is_run_of_a_token)
}

fn is_run_of_a_token(run: &str) -> bool {
    run.len() >= RUN
        && run.bytes().any(|b| b.is_ascii_digit())
        && run.bytes().any(|b| b.is_ascii_alphabetic())
        && !run.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_in_bytes_and_a_text_at_the_limit_is_kept() {
        assert!(check(&"a".repeat(NOTES_MAX)).is_ok());
        let over = check(&"a".repeat(NOTES_MAX + 1)).unwrap_err();
        assert!(over.contains("65536"), "{over}");
    }

    #[test]
    fn a_refusal_does_not_repeat_the_secret() {
        let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let refused = check(&format!("token {secret}")).unwrap_err();
        assert!(!refused.contains("ghp_"), "{refused}");
    }

    #[test]
    fn each_prefix_of_a_token_is_found() {
        for token in [
            "sk-ant-oat01-abcdefghijkl",
            "github_pat_11ABCDEFG0abcdefghijkl",
            "glpat-abcdefghijklmnopqrstu",
            "xoxb-123456789012-abcdefgh",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert!(check(&format!("x {token} y")).is_err(), "{token}");
        }
        assert!(check("the sk-learn skill and the ghp_ prefix").is_ok());
    }
}
