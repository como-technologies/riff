//! The jobs of a workflow of GitHub Actions that a required check needs
//! (01M78TQ943A6KJQ8JD9PAWEYDW).
//!
//! # Design
//!
//! The ruleset of the default branch names the checks that a merge needs,
//! for example `Gate`. The job of such a check can need other jobs. The
//! Gate of riff needs `Plan`, `Build`, the tests and `macOS`, and it fails
//! when one of them fails. So a failed run of such a job is a failed
//! required check, although the ruleset does not name it
//! ([`crate::rollout::Pull::parts`]). The wave fixes it as it fixes a
//! failed Gate, also while the Gate waits for the other jobs. A failed
//! job that no required check needs, for example `Crate audit`, holds
//! nothing ([`crate::rollout::Stuck::Optional`]).
//!
//! ```mermaid
//! flowchart LR
//!     P[Plan] --> B[Build] --> T["Tests 1/4 .. 4/4"] --> G[Gate]
//!     P --> M[macOS] --> G
//!     P --> G
//!     B --> G
//!     P --> A["Crate audit"]
//!     G -. "the ruleset requires it" .-> R["the merge"]
//! ```
//!
//! The look reads the workflow of a failed run, at the head commit of the
//! pull request, with two calls of `gh` ([`read`]). riff reads only what
//! it needs of the YAML ([`jobs`]): the jobs under `jobs:`, and the
//! `name:` and the `needs:` of each. A check run has the name of its job,
//! with the values of each expression `${{ … }}`, and the values of the
//! matrix in brackets when the name has no expression ([`Job::runs`]).

use anyhow::Result;

use crate::pr::Gh;

/// A job of a workflow ([`jobs`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Job {
    /// The ID of the job: its key under `jobs:`.
    pub id: String,
    /// The `name:` of the job, when it has one. It can hold expressions,
    /// for example `Tests ${{ matrix.part }}`.
    pub name: Option<String>,
    /// The IDs of the jobs that it needs: its `needs:`.
    pub needs: Vec<String>,
}

impl Job {
    /// True when `check` is the name of a check run of this job. The
    /// name of the job, or its ID when it has no name, gives the name of
    /// the check run. An expression `${{ … }}` of the name matches any
    /// text. GitHub adds the values of the matrix in brackets to a name
    /// with no expression, and the name of each job of a reusable
    /// workflow after ` / `.
    ///
    /// ```
    /// use riff::workflow::Job;
    ///
    /// let job = |id: &str, name: Option<&str>| Job { id: id.into(), name: name.map(Into::into), needs: vec![] };
    /// assert!(job("macos", Some("macOS")).runs("macOS"));
    /// assert!(!job("macos", Some("macOS")).runs("macos"));
    /// assert!(job("audit", None).runs("audit"));
    /// let tests = job("test", Some("Tests ${{ matrix.part }}"));
    /// assert!(tests.runs("Tests 2/4"));
    /// assert!(!tests.runs("Test 2/4"));
    /// assert!(job("test", None).runs("test (1/4, ubuntu)"));
    /// assert!(job("checks", Some("checks")).runs("checks / docs"));
    /// assert!(!job("checks", Some("checks")).runs("checks2"));
    /// ```
    pub fn runs(&self, check: &str) -> bool {
        let template = self.name.as_deref().unwrap_or(&self.id);
        let matrix = check
            .strip_suffix(')')
            .and_then(|c| c.rsplit_once(" ("))
            .map(|(name, _)| name);
        let caller = check.split_once(" / ").map(|(name, _)| name);
        [Some(check), matrix, caller]
            .into_iter()
            .flatten()
            .any(|name| matches(template, name))
    }
}

/// True when `name` is `template` with any text for each expression
/// `${{ … }}` of it.
fn matches(template: &str, name: &str) -> bool {
    let mut pieces: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some((before, after)) = rest.split_once("${{") {
        pieces.push(before);
        rest = after.split_once("}}").map_or("", |(_, tail)| tail);
    }
    pieces.push(rest);
    let [first, middle @ .., last] = pieces.as_slice() else {
        return template == name;
    };
    let Some(mut left) = name.strip_prefix(first) else {
        return false;
    };
    for piece in middle {
        match left.find(piece) {
            Some(at) => left = &left[at + piece.len()..],
            None => return false,
        }
    }
    left.len() >= last.len() && left.ends_with(last)
}

/// The jobs of the workflow `text`, in order. It reads the keys under the
/// key `jobs:` at the start of a line, and of each job its `name:` and
/// its `needs:`: one ID, a list in brackets, or a list of lines `- ID`.
/// It reads no other YAML: a text that it does not know gives fewer jobs,
/// or none.
///
/// ```
/// use riff::workflow::{jobs, Job};
///
/// let text = r#"
/// name: CI
/// on: [pull_request]
/// jobs:
///   plan:
///     name: Plan # the plan of the run
///     runs-on: ubuntu-latest
///   test:
///     name: "Tests ${{ matrix.part }}"
///     needs: [plan, 'build']
///     steps:
///       - name: Not a job
///         run: |
///           echo needs: x
///   macos:
///     needs: plan
///   gate:
///     name: Gate
///     needs:
///     - plan
///     - test
///     - macos
/// "#;
/// let job = |id: &str, name: Option<&str>, needs: &[&str]| Job {
///     id: id.into(),
///     name: name.map(Into::into),
///     needs: needs.iter().map(|n| (*n).into()).collect(),
/// };
/// assert_eq!(
///     jobs(text),
///     [
///         job("plan", Some("Plan"), &[]),
///         job("test", Some("Tests ${{ matrix.part }}"), &["plan", "build"]),
///         job("macos", None, &["plan"]),
///         job("gate", Some("Gate"), &["plan", "test", "macos"]),
///     ]
/// );
/// assert!(jobs("name: CI\non: push\n").is_empty());
/// assert!(jobs("").is_empty());
/// ```
pub fn jobs(text: &str) -> Vec<Job> {
    // Each line with its indent and its content, with no comment and no
    // empty line.
    let lines: Vec<(usize, &str)> = text
        .lines()
        .map(no_comment)
        .filter(|line| !line.trim().is_empty())
        .map(|line| (line.len() - line.trim_start().len(), line.trim()))
        .collect();
    let Some(start) = lines
        .iter()
        .position(|&(at, line)| at == 0 && line == "jobs:")
    else {
        return Vec::new();
    };
    let body: Vec<(usize, &str)> = lines[start + 1..]
        .iter()
        .copied()
        .take_while(|&(at, _)| at > 0)
        .collect();
    let Some(&(top, _)) = body.first() else {
        return Vec::new();
    };
    let mut jobs: Vec<Job> = Vec::new();
    // The job that the loop reads, and the indent of its keys.
    let mut job: Option<Job> = None;
    let mut keys: Option<usize> = None;
    let mut i = 0;
    while i < body.len() {
        let (at, line) = body[i];
        i += 1;
        if at <= top {
            jobs.extend(job.take());
            keys = None;
            if let Some((id, "")) = key_value(line).filter(|_| at == top) {
                job = Some(Job {
                    id: id.to_owned(),
                    ..Job::default()
                });
            }
            continue;
        }
        let Some(job) = job.as_mut() else {
            continue;
        };
        if *keys.get_or_insert(at) != at || line.starts_with('-') {
            continue;
        }
        match key_value(line) {
            Some(("name", value)) => job.name = Some(unquote(value).to_owned()),
            Some(("needs", "")) => {
                while let Some(&(item_at, item)) = body.get(i) {
                    let Some(need) = item.strip_prefix('-').filter(|_| item_at >= at) else {
                        break;
                    };
                    job.needs.push(unquote(need.trim()).to_owned());
                    i += 1;
                }
            }
            Some(("needs", value)) if value.starts_with('[') => {
                let mut list = value.to_owned();
                while !list.contains(']') {
                    let Some(&(_, more)) = body.get(i) else {
                        break;
                    };
                    list.push(' ');
                    list.push_str(more);
                    i += 1;
                }
                let inner = list.trim_start_matches('[');
                let inner = inner.split(']').next().unwrap_or_default();
                job.needs.extend(
                    inner
                        .split(',')
                        .map(|need| unquote(need.trim()))
                        .filter(|need| !need.is_empty())
                        .map(str::to_owned),
                );
            }
            Some(("needs", value)) => job.needs.push(unquote(value).to_owned()),
            _ => {}
        }
    }
    jobs.extend(job);
    jobs
}

/// The line with no comment: a `#` at its start, or after a space, and
/// not in quotes, starts a comment.
fn no_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut before = ' ';
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (None, '#') if before.is_whitespace() => return &line[..at],
            (None, '"' | '\'') if before.is_whitespace() || before == '[' || before == ',' => {
                quote = Some(c);
            }
            (Some(open), _) if c == open => quote = None,
            _ => {}
        }
        before = c;
    }
    line
}

/// The key and the value of a line `KEY: VALUE` or `KEY:`, with no quotes
/// around the key.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = match line.split_once(": ") {
        Some((key, value)) => (key, value.trim()),
        None => (line.strip_suffix(':')?, ""),
    };
    Some((unquote(key.trim()), value))
}

/// The text with no quotes around it.
fn unquote(text: &str) -> &str {
    ['"', '\'']
        .into_iter()
        .find_map(|q| text.strip_prefix(q).and_then(|t| t.strip_suffix(q)))
        .unwrap_or(text)
}

/// The jobs of `jobs` that a job of a check of `required` needs, also
/// through other jobs, in the order of `jobs`
/// (01M78TQ943A6KJQ8JD9PAWEYDW). A job of a required check that no other
/// one needs is not in the list.
///
/// ```
/// use riff::workflow::{jobs, needed};
///
/// let text = "jobs:\n  plan:\n    name: Plan\n  build:\n    needs: plan\n  \
///             test:\n    needs: build\n  macos:\n    name: macOS\n    needs: plan\n  \
///             audit:\n    name: Crate audit\n  gate:\n    name: Gate\n    needs: [test, macos]\n";
/// let jobs = jobs(text);
/// let ids = |required: &[&str]| -> Vec<String> {
///     let required: Vec<String> = required.iter().map(|r| (*r).into()).collect();
///     needed(&jobs, &required).iter().map(|j| j.id.clone()).collect()
/// };
/// assert_eq!(ids(&["Gate", "Hygiene"]), ["plan", "build", "test", "macos"]);
/// assert_eq!(ids(&["macOS"]), ["plan"]);
/// assert!(ids(&["Crate audit"]).is_empty());
/// assert!(ids(&["Hygiene"]).is_empty());
/// ```
pub fn needed<'a>(jobs: &'a [Job], required: &[String]) -> Vec<&'a Job> {
    let mut wanted: Vec<&str> = jobs
        .iter()
        .filter(|job| required.iter().any(|check| job.runs(check)))
        .flat_map(|job| job.needs.iter().map(String::as_str))
        .collect();
    let mut found: Vec<&str> = Vec::new();
    while let Some(id) = wanted.pop() {
        if found.contains(&id) {
            continue;
        }
        found.push(id);
        if let Some(job) = jobs.iter().find(|job| job.id == id) {
            wanted.extend(job.needs.iter().map(String::as_str));
        }
    }
    jobs.iter()
        .filter(|job| found.contains(&job.id.as_str()))
        .collect()
}

/// The jobs of the workflow of the run `run` of GitHub Actions in `repo`
/// (`OWNER/REPO`), at `commit`, with two calls of `gh`: the path of the
/// workflow of the run, and its file at the commit. Neither changes, so a
/// `gh` that shares its reads asks one time in [`Gh::LONG`]. A run or a
/// file that GitHub does not find gives none, and so does a workflow that
/// is not a file of `.github/workflows`. A fault of `gh` is an error.
pub fn read(gh: &Gh, repo: &str, run: u64, commit: &str) -> Result<Vec<Job>> {
    let api = format!("repos/{repo}/actions/runs/{run}");
    let Some(path) = found(gh.read_within(&["api", &api, "--jq", ".path"], Gh::LONG))? else {
        return Ok(Vec::new());
    };
    let path = path.trim();
    let path = path.split_once('@').map_or(path, |(file, _)| file);
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./".contains(c);
    if !path.starts_with(".github/workflows/") || path.contains("..") || !path.chars().all(safe) {
        return Ok(Vec::new());
    }
    if commit.is_empty() || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(Vec::new());
    }
    let api = format!("repos/{repo}/contents/{path}?ref={commit}");
    let args = ["api", "-H", "Accept: application/vnd.github.raw+json", &api];
    Ok(found(gh.read_within(&args, Gh::LONG))?
        .map(|text| jobs(&text))
        .unwrap_or_default())
}

/// The reply of a read, or `None` when GitHub answered 404.
fn found(reply: Result<String>) -> Result<Option<String>> {
    match reply {
        Ok(text) => Ok(Some(text)),
        Err(e) if format!("{e:#}").contains("HTTP 404") => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Gate of the CI of this repository needs `macOS` and the tests,
    /// and not `Crate audit` (#2022). When the workflow changes its form,
    /// the look must still find the parts of the Gate.
    #[test]
    fn the_gate_of_this_repository_needs_macos() {
        let jobs = jobs(include_str!("../../../.github/workflows/ci.yml"));
        let parts = needed(&jobs, &["Gate".to_owned(), "Hygiene".to_owned()]);
        let part = |check: &str| parts.iter().any(|job| job.runs(check));
        assert!(part("macOS"), "{parts:?}");
        assert!(part("Build"));
        assert!(part("Tests 3/4"));
        assert!(!part("Crate audit"));
        assert!(!part("Static riff"));
        assert!(!part("Gate"));
        assert!(jobs.iter().any(|job| job.runs("Crate audit")));
    }

    /// A key at the indent of the jobs that is no job, as `a: {}`, ends
    /// the job before it.
    #[test]
    fn a_key_that_is_no_job_ends_the_job_before_it() {
        let text = "jobs:\n  a:\n    name: A\n  b: {name: B}\n    needs: x\n";
        let job = Job {
            id: "a".into(),
            name: Some("A".into()),
            needs: vec![],
        };
        assert_eq!(jobs(text), [job]);
    }

    #[test]
    fn a_text_with_no_jobs_or_odd_lines_gives_what_it_can() {
        assert!(jobs("jobs: {}\n").is_empty());
        assert!(jobs("jobs:\n").is_empty());
        // A list in brackets on more lines, and a key in quotes.
        let text = "jobs:\n  \"gate\":\n    needs: [\n      a,\n      b ]\n    name: 'Gate'\n";
        assert_eq!(
            jobs(text),
            [Job {
                id: "gate".into(),
                name: Some("Gate".into()),
                needs: vec!["a".into(), "b".into()],
            }]
        );
        // A `#` in quotes is no comment.
        let text = "jobs:\n  a:\n    name: \"Build #1\" # the first\n";
        assert_eq!(jobs(text)[0].name.as_deref(), Some("Build #1"));
        // A list of lines under the key, indented.
        let text = "jobs:\n  a:\n    needs:\n      - b\n      - \"c\"\n    name: A\n";
        let job = &jobs(text)[0];
        assert_eq!(job.needs, ["b", "c"]);
        assert_eq!(job.name.as_deref(), Some("A"));
    }

    #[test]
    fn a_template_with_more_expressions_matches_in_order() {
        assert!(matches("${{ a }} on ${{ b }}", "x on y"));
        assert!(matches("t ${{ a }}", "t "));
        assert!(!matches("a ${{ x }} b", "a b"));
        assert!(!matches("Gate", "Gate 2"));
    }
}
