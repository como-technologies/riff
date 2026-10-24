//! The signed server of the integration tests ([`common::Signed`]):
//! a command that sends a call with no token fails its test.

use crate::common;
use riff_client::api::Api;
use riff_core::name::SessionUri;

fn me() -> SessionUri {
    "riff://brett@pangolin".parse().unwrap()
}

/// The control: a client from `Api::new`, as a command that forgets
/// `Api::signed_in` makes it, sends no token. The server refuses the
/// call and notes it, so the test of that command fails.
#[tokio::test]
async fn a_call_with_no_token_is_noted() {
    let server = common::start_signed().await;
    let bare = Api::new(&server);
    assert!(bare.who(&me(), false).await.is_err());
    assert_eq!(server.take_unsigned(), ["/v1/who"]);
}

/// A client that `common::api` makes sends the token of the person.
#[tokio::test]
async fn a_call_with_a_token_is_not_noted() {
    let server = common::start_signed().await;
    common::api(&server, "brett", None)
        .who(&me(), false)
        .await
        .unwrap();
    assert!(server.unsigned().is_empty());
}

/// The test ends with a call with no token that it did not take: the
/// server fails it.
#[tokio::test]
#[should_panic(expected = "a command sent these calls with no token")]
async fn a_test_that_leaves_a_call_with_no_token_fails() {
    let server = common::start_signed().await;
    let _ = Api::new(&server).who(&me(), false).await;
}
