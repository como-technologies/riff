//! `riff lead start` makes the new session the lead, and `whoami` of a
//! lead container that is not the lead says so in its first line
//! (#1301). On a real riff-server.

use riff::tools::Tools;
use riff_client::api::Api;
use riff_core::name::SessionUri;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// The text of `whoami` of the lead container `id`.
async fn whoami(api: &Api, id: &str) -> String {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(api.clone(), uri(id)).in_lead_container();
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let result = client
        .call_tool(CallToolRequestParams::new("whoami".to_owned()))
        .await
        .unwrap();
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect()
}

#[tokio::test]
async fn whoami_of_a_lead_container_that_is_not_the_lead_says_so_first() {
    let api = Api::new(&start_server().await);
    api.register(&uri("94adc2ec")).await.unwrap();
    api.lead(&uri("94adc2ec")).await.unwrap();
    api.register(&uri("a17678d3")).await.unwrap();
    let text = whoami(&api, "a17678d3").await;
    let first = text.lines().next().unwrap();
    assert!(first.starts_with("You are not the lead"), "{text}");
    assert!(first.contains("(94adc2ec) is the lead"), "{text}");
}

#[tokio::test]
async fn whoami_of_the_lead_container_that_is_the_lead_has_no_such_line() {
    let api = Api::new(&start_server().await);
    api.register(&uri("94adc2ec")).await.unwrap();
    api.lead(&uri("94adc2ec")).await.unwrap();
    api.register(&uri("a17678d3")).await.unwrap();
    // What `riff lead start` does after the start of the session.
    let reply = api.lead(&uri("a17678d3")).await.unwrap();
    assert!(reply.replaced.is_some());
    let text = whoami(&api, "a17678d3").await;
    assert!(!text.contains("not the lead"), "{text}");
}
