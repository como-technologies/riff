//! The HTTP client for `riff-server`.

use anyhow::{Context, Result, bail};
use futures::{Stream, StreamExt};
use riff_core::name::{SessionName, ThreadName};
use riff_core::wire::{
    Claim, ClaimReply, Membership, Message, Post, Posted, Read, ReadReply, Register, SessionInfo,
    Tailed, Tell, ThreadInfo, Threads, ThreadsReply, Wake, Who, WhoReply,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The server that `riff` uses when nothing else is set.
pub const DEFAULT_SERVER: &str = "http://127.0.0.1:7878";

#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
}

impl Api {
    pub fn new(base: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    pub async fn register(&self, name: &SessionName) -> Result<()> {
        self.call("register", &Register { name: name.clone() })
            .await
    }

    pub async fn who(&self) -> Result<Vec<SessionInfo>> {
        let reply: WhoReply = self.call("who", &Who {}).await?;
        Ok(reply.sessions)
    }

    pub async fn threads(&self, name: &SessionName) -> Result<Vec<ThreadInfo>> {
        let reply: ThreadsReply = self
            .call("threads", &Threads { name: name.clone() })
            .await?;
        Ok(reply.threads)
    }

    pub async fn join(&self, name: &SessionName, thread: &ThreadName) -> Result<()> {
        self.call("join", &membership(name, thread)).await
    }

    pub async fn leave(&self, name: &SessionName, thread: &ThreadName) -> Result<()> {
        self.call("leave", &membership(name, thread)).await
    }

    pub async fn post(
        &self,
        from: &SessionName,
        thread: &ThreadName,
        body: &str,
    ) -> Result<Posted> {
        let request = Post {
            from: from.clone(),
            thread: thread.clone(),
            body: body.to_owned(),
        };
        self.call("post", &request).await
    }

    pub async fn tell(&self, from: &SessionName, to: &SessionName, body: &str) -> Result<Posted> {
        let request = Tell {
            from: from.clone(),
            to: to.clone(),
            body: body.to_owned(),
        };
        self.call("tell", &request).await
    }

    pub async fn read(
        &self,
        name: &SessionName,
        thread: &ThreadName,
        all: bool,
    ) -> Result<Vec<Message>> {
        let request = Read {
            name: name.clone(),
            thread: thread.clone(),
            all,
        };
        let reply: ReadReply = self.call("read", &request).await?;
        Ok(reply.messages)
    }

    pub async fn claim(
        &self,
        name: &SessionName,
        thread: &ThreadName,
        item: &str,
    ) -> Result<ClaimReply> {
        self.call("claim", &claim(name, thread, item)).await
    }

    pub async fn release(&self, name: &SessionName, thread: &ThreadName, item: &str) -> Result<()> {
        self.call("release", &claim(name, thread, item)).await
    }

    /// The wakes for one session. The session is live while the stream
    /// is open.
    pub async fn watch(&self, name: &SessionName) -> Result<impl Stream<Item = Result<Wake>>> {
        self.events("watch", &[("name", name.to_string())]).await
    }

    /// Each new message in one thread.
    pub async fn tail(&self, thread: &ThreadName) -> Result<impl Stream<Item = Result<Tailed>>> {
        self.events("tail", &[("thread", thread.to_string())]).await
    }

    async fn call<Req: Serialize, Rep: DeserializeOwned>(
        &self,
        op: &str,
        request: &Req,
    ) -> Result<Rep> {
        let response = self
            .http
            .post(format!("{}/v1/{op}", self.base))
            .json(request)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("{op} failed ({status}): {text}");
        }
        Ok(response.json().await?)
    }

    /// Reads a server-sent event stream and parses each `data:` line.
    async fn events<T: DeserializeOwned>(
        &self,
        op: &str,
        query: &[(&str, String)],
    ) -> Result<impl Stream<Item = Result<T>> + use<T>> {
        let response = self
            .http
            .get(format!("{}/v1/{op}", self.base))
            .query(query)
            .send()
            .await
            .with_context(|| format!("cannot reach riff-server at {}", self.base))?
            .error_for_status()?;
        let mut buffer = String::new();
        let lines = response.bytes_stream().flat_map(move |chunk| {
            let lines: Vec<Result<String>> = match chunk {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    let mut lines = Vec::new();
                    while let Some(end) = buffer.find('\n') {
                        let line: String = buffer.drain(..=end).collect();
                        lines.push(Ok(line.trim_end().to_owned()));
                    }
                    lines
                }
                Err(e) => vec![Err(e.into())],
            };
            futures::stream::iter(lines)
        });
        Ok(lines.filter_map(|line| async move {
            match line {
                Ok(line) => line
                    .strip_prefix("data:")
                    .map(|data| serde_json::from_str(data.trim()).map_err(Into::into)),
                Err(e) => Some(Err(e)),
            }
        }))
    }
}

fn membership(name: &SessionName, thread: &ThreadName) -> Membership {
    Membership {
        name: name.clone(),
        thread: thread.clone(),
    }
}

fn claim(name: &SessionName, thread: &ThreadName, item: &str) -> Claim {
    Claim {
        name: name.clone(),
        thread: thread.clone(),
        item: item.to_owned(),
    }
}
