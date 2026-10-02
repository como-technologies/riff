//! Makes the objects of a riff-server of v0.8.0, for the import test of
//! go-live. Run it in the tree of v0.8.0: `cargo run --example fixture -- DIR`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, Post, RiffState, Status};
use riff_server::state::{Object, State};
use riff_server::token::Tokens;
use serde_json::json;

/// The time of the save, in milliseconds since the Unix epoch.
const SAVED_MS: u64 = 1_790_000_000_000;

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn main() {
    let out = std::env::args().nth(1).expect("the output directory");
    let out = Path::new(&out);
    std::fs::create_dir_all(out.join("threads")).unwrap();

    let t0 = Instant::now();
    let day = Duration::from_secs(24 * 60 * 60);
    let min = Duration::from_secs(60);
    // The save is 40 days after the first call.
    let saved = t0 + 40 * day;
    let ms = |t: Instant| SAVED_MS - u64::try_from((saved - t).as_millis()).unwrap();
    let wall = UNIX_EPOCH + Duration::from_millis(SAVED_MS);

    // The people and the sign-ins.
    let mut tokens = Tokens::default();
    let soon = saved - 2 * day;
    let mike = tokens
        .admit("mike@comotechnologies.io", false, &[], "jkt-mike", soon)
        .unwrap();
    tokens.invite("brett@comotechnologies.io").unwrap();
    let brett = tokens
        .admit("brett@comotechnologies.io", false, &[], "jkt-brett", soon)
        .unwrap();
    tokens.add_admin("brett@comotechnologies.io").unwrap();
    tokens.invite("gone@example.com").unwrap();
    let gone = tokens
        .admit("gone@example.com", false, &[], "jkt-gone", soon)
        .unwrap();
    tokens.remove("gone@example.com").unwrap();
    tokens.invite("new@comotechnologies.io").unwrap();
    tokens
        .take_owner("brett", &[], 7 * day, saved - day)
        .unwrap();
    // Mike used one refresh token, and has a session pair.
    let mike_now = tokens
        .refresh(&mike.refresh_token, "jkt-mike", saved - 2 * min)
        .unwrap();
    let mike_session = tokens
        .for_session(&mike_now.access_token, "jkt-mike", "m1", saved - min)
        .unwrap();

    // The sessions.
    let m1 = uri("riff://mike@pangolin/como-technologies/riff?session=m1");
    let m2 = uri("riff://mike@pangolin/como-technologies/riff?session=m2#issue-341");
    let m3 = uri("riff://mike@thelio/como-technologies/riff?session=m3");
    let m4 = uri("riff://mike@thelio/como-technologies/riff?session=m4");
    let m5 = uri("riff://mike@thelio/como-technologies/riff?session=m5#issue-9");
    let old = uri("riff://mike@thelio/como-technologies/riff?session=old");
    let b1 = uri("riff://brett@kadomony/como-technologies/strata?session=b1");
    let b2 = uri("riff://brett@kadomony/como-technologies/strata?session=b2#issue-7");
    let riff: ThreadName = "como-technologies/riff".parse().unwrap();
    let strata: ThreadName = "como-technologies/strata".parse().unwrap();
    let design: ThreadName = "design".parse().unwrap();

    let mut state = State::default();
    // A session with no sign of life for 40 days: the load drops it.
    state.register(&old, t0);
    state.riff(&old, Some(RiffState::Running), t0).unwrap();
    state.claim(&old, &riff, "issue-1", t0).unwrap();

    let start = saved - 2 * 60 * min;
    for me in [&m1, &m2, &m3, &m4, &m5, &b1, &b2] {
        state.register(me, start);
    }
    state.lead(&m1, start).unwrap();
    state.worker(m2.who(), true);
    state.worker(b2.who(), true);
    state.set_idle(Some(2), Some(600));
    state.claim(&m2, &riff, "issue-341", start).unwrap();
    state.claim(&b2, &strata, "issue-7", start).unwrap();
    state.claim(&m5, &riff, "issue-9", start).unwrap();
    state.join(&m1, &design, start);
    state.join(&b1, &design, start);

    // 205 notes in the riff thread: the import keeps the last 200.
    for n in 1..=205 {
        let mut post = Post::new(&m1, Some(riff.clone()), vec![], &format!("note {n}"));
        post.kind = Kind::Note;
        state.post(post, start, ms(start)).unwrap();
    }
    // The worker reads them, then two more messages come.
    state.read(&m2, &riff, false, start).unwrap();
    let to_mike = vec!["user=mike".parse().unwrap()];
    let board = Post::new(&m1, Some(riff.clone()), to_mike, "the board of Wave 18");
    state.post(board, start + min, ms(start + min)).unwrap();
    let verify = Post::new(&m2, Some(riff.clone()), vec![], "verify request: issue-341");
    state
        .post(verify, start + 2 * min, ms(start + 2 * min))
        .unwrap();
    let hello = Post::new(&b1, Some(strata.clone()), vec![], "strata: the plan");
    state.post(hello, start + min, ms(start + min)).unwrap();
    let idea = Post::new(&b1, Some(design.clone()), vec![], "an idea for the design");
    state.post(idea, start + min, ms(start + min)).unwrap();
    // Direct messages: one between two live sessions, one between two
    // sessions that end.
    let request = Post::new(
        &b1,
        None,
        vec!["session=b2".parse().unwrap()],
        "request: claim issue-7",
    );
    state.post(request, start + min, ms(start + min)).unwrap();
    let last = Post::new(&m3, None, vec!["session=m4".parse().unwrap()], "bye");
    state.post(last, start + min, ms(start + min)).unwrap();
    let status = Status {
        step: "issue-341: writes the import".into(),
        blocked: None,
    };
    state
        .set_status(&m2, status, start + 3 * min, ms(start + 3 * min))
        .unwrap();
    state.end(&m3, start + 4 * min);
    state.end(&m4, start + 4 * min);

    // The last sign of life of m5 is 2 hours before the save: its claim
    // ended. Each other session is live one minute before the save.
    for me in [&m1, &m2, &b1, &b2] {
        state.alive(me, saved - min);
    }

    let mut sessions = Vec::new();
    let mut threads = Vec::new();
    for (object, bytes) in state.changes(saved, SAVED_MS) {
        let name = object.name();
        std::fs::write(out.join(&name), &bytes).unwrap();
        match object {
            Object::Sessions => sessions = bytes,
            Object::Thread(_) => threads.push((name, bytes)),
        }
    }
    std::fs::write(out.join("tokens"), tokens.to_bytes(saved, wall)).unwrap();

    // What the old server shows after a restart at the time of the save.
    let listed = threads.iter().map(|(n, b)| (n.as_str(), b.as_slice()));
    let mut loaded = State::load(Some(&sessions), listed, saved, SAVED_MS).unwrap();
    let who: Vec<String> = loaded
        .who(saved, SAVED_MS, true)
        .iter()
        .map(|s| s.uri.to_string())
        .collect();
    let workers: Vec<String> = loaded
        .who(saved, SAVED_MS, true)
        .iter()
        .filter(|s| s.worker)
        .map(|s| s.uri.who().to_string())
        .collect();
    let mut unread = BTreeMap::new();
    let mut messages = BTreeMap::new();
    for me in [&m1, &m2, &m3, &m4, &m5, &b1, &b2] {
        let mut of = BTreeMap::new();
        for info in loaded.threads(me, saved) {
            of.insert(info.thread.to_string(), info.unread);
            let all = loaded.read(me, &info.thread, true, saved).unwrap();
            let keep = all.len().saturating_sub(200);
            let kept: Vec<_> = all[keep..]
                .iter()
                .map(|m| json!({"seq": m.seq, "body": m.body, "from": m.from.who().to_string()}))
                .collect();
            messages.insert(info.thread.to_string(), kept);
        }
        unread.insert(me.who().to_string(), of);
    }
    let reply = loaded.riff(&m1, None, saved).unwrap();
    let facts = json!({
        "saved_ms": SAVED_MS,
        "riff_id": tokens.riff_id(),
        "owner": tokens.owner(),
        "who": who,
        "workers": workers,
        "unread": unread,
        "messages": messages,
        "riff": reply.state,
        "idle": loaded.idle(),
        "refresh": {
            "mike": {"jkt": "jkt-mike", "token": mike_now.refresh_token},
            "mike_used": {"jkt": "jkt-mike", "token": mike.refresh_token},
            "mike_session": {"jkt": "jkt-mike", "token": mike_session.refresh_token},
            "brett": {"jkt": "jkt-brett", "token": brett.refresh_token},
            "gone": {"jkt": "jkt-gone", "token": gone.refresh_token},
        },
    });
    let facts = serde_json::to_string_pretty(&facts).unwrap();
    std::fs::write(out.join("facts.json"), facts + "\n").unwrap();
    let _ = SystemTime::now();
}
