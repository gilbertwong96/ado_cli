//! The client against a real socket and the mock Azure DevOps server: URL shape,
//! headers, and the spec §6.2 classification of every response the contract names.

use std::net::TcpListener;

use ado_core::client::Client;
use ado_core::config::AuthMethod;
use ado_core::credentials::Credentials;
use ado_core::env::{ENV_SERVER, MapEnv};
use ado_core::error::ErrorCode;
use ado_testkit::{MockResponse, MockServer};
use serde_json::json;

const SIGN_IN_MESSAGE: &str = "API redirected to sign-in page. Run 'ado login' to authenticate.";
const NO_LOCATION_MESSAGE: &str =
    "API redirected without a Location header. Run 'ado login' to authenticate.";
const NOT_FOUND_MESSAGE: &str =
    "Resource not found. Check the project/repo/build ID and your permissions.";

fn credentials(org: &str) -> Credentials {
    Credentials {
        org: org.to_owned(),
        method: AuthMethod::Pat,
        token: "pat".to_owned(),
    }
}

fn client_with(server: Option<&str>) -> Client {
    let mut env = MapEnv::new();
    if let Some(server) = server {
        env = env.set(ENV_SERVER, server);
    }

    Client::from_env(&credentials("myorg"), &env).expect("a client")
}

fn client_for(server: &MockServer) -> Client {
    client_with(Some(server.base_url()))
}

/// The path the mock receives for an API path: a self-hosted server gets the org
/// inserted after the host, exactly as `AdoCli.CLI.TestHelper.api/1` does.
fn api(path: &str) -> String {
    format!("/myorg{path}")
}

#[test]
fn url_uses_visualstudio_host_for_cloud() {
    assert_eq!(
        client_with(None).url_for("/_apis/projects", &[]),
        "https://myorg.visualstudio.com/_apis/projects?api-version=7.1"
    );
}

#[test]
fn url_prefixes_org_for_self_hosted() {
    assert_eq!(
        client_with(Some("https://server.test")).url_for("/_apis/projects", &[]),
        "https://server.test/myorg/_apis/projects?api-version=7.1"
    );
}

#[test]
fn caller_params_override_api_version() {
    let params = [
        ("api-version".to_owned(), "5.0".to_owned()),
        ("$top".to_owned(), "10".to_owned()),
    ];

    assert_eq!(
        client_with(None).url_for("/_apis/projects", &params),
        "https://myorg.visualstudio.com/_apis/projects?api-version=5.0&%24top=10"
    );
}

#[test]
fn pat_is_sent_as_basic_auth() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/projects"),
        MockResponse::json(200, json!({})),
    );
    let client = client_for(&server);

    client
        .get("/_apis/projects", &[])
        .expect("the mock answers");

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, api("/_apis/projects"));
    assert_eq!(received[0].query, "api-version=7.1");
    assert_eq!(received[0].body, None);
    assert_eq!(
        received[0].header("authorization"),
        Some("Basic OnBhdA=="),
        "headers: {:?}",
        received[0].headers
    );
}

#[test]
fn get_decodes_json() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/projects"),
        MockResponse::from_fixture("projects_list"),
    );
    let client = client_for(&server);

    let body = client
        .get("/_apis/projects", &[])
        .expect("the mock answers");

    assert_eq!(body["count"], 2);
    assert_eq!(body["value"][0]["name"], "Alpha");
}

#[test]
fn list_unwraps_the_value_array() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/projects"),
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "GET",
        &api("/_apis/tags"),
        MockResponse::json(200, json!([{"name": "one"}, {"name": "two"}])),
    );
    let client = client_for(&server);

    let projects = client
        .list("/_apis/projects", &[])
        .expect("the mock answers");
    assert_eq!(projects.as_array().expect("the value array").len(), 2);

    let tags = client
        .list("/_apis/tags", &[])
        .expect("a bare array passes through");
    assert_eq!(tags.as_array().expect("the bare array").len(), 2);
}

#[test]
fn write_methods_send_a_json_body_and_delete_accepts_an_empty_204() {
    let server = MockServer::start();
    server.expect(
        "POST",
        &api("/_apis/projects"),
        MockResponse::json(200, json!({"id": "p1"})),
    );
    server.expect(
        "PATCH",
        &api("/_apis/projects/p1"),
        MockResponse::json(200, json!({"id": "p1"})),
    );
    server.expect(
        "PUT",
        &api("/_apis/build/builds/1/tags"),
        MockResponse::json(200, json!(["tag"])),
    );
    server.expect(
        "DELETE",
        &api("/_apis/projects/p1"),
        MockResponse {
            status: 204,
            body: Vec::new(),
            headers: Vec::new(),
        },
    );
    let client = client_for(&server);
    let body = json!({"name": "New"});

    let created = client
        .post("/_apis/projects", &body, &[])
        .expect("post answers");
    assert_eq!(created["id"], "p1");
    client
        .patch("/_apis/projects/p1", &body, &[])
        .expect("patch answers");
    client
        .put("/_apis/build/builds/1/tags", &json!(["tag"]), &[])
        .expect("put answers");
    client
        .delete("/_apis/projects/p1", &[])
        .expect("204 is an accepted delete");

    let received = server.received();
    let methods: Vec<&str> = received
        .iter()
        .map(|request| request.method.as_str())
        .collect();
    assert_eq!(methods, ["POST", "PATCH", "PUT", "DELETE"]);
    assert_eq!(received[0].body.as_deref(), Some(r#"{"name":"New"}"#));
    assert_eq!(received[0].header("content-type"), Some("application/json"));
    assert_eq!(received[3].body, None);
}

#[test]
fn not_found_maps_to_the_contract_code() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/projects/missing"),
        MockResponse::from_fixture("error_404").with_status(404),
    );
    let client = client_for(&server);

    let error = client
        .get("/_apis/projects/missing", &[])
        .expect_err("404 is an error");

    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.status, Some(404));
    assert_eq!(error.message, NOT_FOUND_MESSAGE);

    let details = error.details.expect("details carry the status and body");
    assert_eq!(details["status"], 404);
    let body = details["body"].as_str().expect("the body is a string");
    assert!(
        body.contains("ProjectDoesNotExistException"),
        "body: {body}"
    );
}

#[test]
fn redirect_maps_to_auth_required() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/sign-in"),
        MockResponse {
            status: 302,
            body: Vec::new(),
            headers: vec![(
                "location".to_owned(),
                "https://login.microsoftonline.com/".to_owned(),
            )],
        },
    );
    server.expect(
        "GET",
        &api("/_apis/no-location"),
        MockResponse {
            status: 302,
            body: Vec::new(),
            headers: Vec::new(),
        },
    );
    let client = client_for(&server);

    let with_location = client
        .get("/_apis/sign-in", &[])
        .expect_err("302 is an error");
    assert_eq!(with_location.code, ErrorCode::AuthRequired);
    assert_eq!(with_location.status, Some(302));
    assert_eq!(with_location.message, SIGN_IN_MESSAGE);

    let without_location = client
        .get("/_apis/no-location", &[])
        .expect_err("302 is an error");
    assert_eq!(without_location.code, ErrorCode::AuthRequired);
    assert_eq!(without_location.status, Some(302));
    assert_eq!(without_location.message, NO_LOCATION_MESSAGE);
}

#[test]
fn connection_refused_maps_to_network_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let address = listener.local_addr().expect("the bound address");
    drop(listener);

    let client = client_with(Some(&format!("http://{address}")));
    let error = client
        .get("/_apis/projects", &[])
        .expect_err("nothing is listening");

    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.status, None);
    assert_eq!(
        error.message,
        "Connection refused. Is the server reachable?"
    );

    let details = error.details.expect("details carry the reason");
    assert_eq!(
        details["reason"].as_str().map(str::is_empty),
        Some(false),
        "details: {details}"
    );
}

#[test]
fn malformed_json_on_200_is_network_error() {
    let server = MockServer::start();
    server.expect(
        "GET",
        &api("/_apis/projects"),
        MockResponse {
            status: 200,
            body: b"this is not JSON".to_vec(),
            headers: Vec::new(),
        },
    );
    let client = client_for(&server);

    let error = client
        .get("/_apis/projects", &[])
        .expect_err("the body cannot be decoded");

    assert_eq!(
        error.code,
        ErrorCode::NetworkError,
        "the current classification calls a decode failure a network error"
    );
    assert_eq!(error.status, None);
    assert!(
        error.message.starts_with("Request failed:"),
        "message: {}",
        error.message
    );
}

/// `get_raw` is the download path: the URL is used verbatim (no `api-version` is
/// merged in), and the body comes back as the bytes the server sent — the zip
/// fixture is not valid UTF-8, so a string body could not carry it (D25).
#[test]
fn get_raw_returns_the_body_bytes_verbatim() {
    let server = MockServer::start();
    server.expect(
        "GET",
        "/blob/drop.zip",
        MockResponse::from_bytes_fixture("artifacts_download.zip"),
    );
    let client = client_for(&server);
    let url = format!("{}/blob/drop.zip", server.base_url());

    let bytes = client.get_raw(&url).expect("the mock answers");

    assert_eq!(
        bytes,
        MockResponse::from_bytes_fixture("artifacts_download.zip").body
    );
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, "/blob/drop.zip");
    assert_eq!(
        received[0].query, "",
        "an absolute URL is requested as given, with no added api-version"
    );
    assert_eq!(received[0].header("authorization"), Some("Basic OnBhdA=="));
}

/// A non-2xx download classifies by its status per spec §6.2 — where the oracle's
/// `get_raw` error shape (a bare `%{status: s}` map, no body) falls through to its
/// network-error text (D25).
#[test]
fn get_raw_classifies_status_errors() {
    let server = MockServer::start();
    server.expect(
        "GET",
        "/blob/missing.zip",
        MockResponse::json(404, json!({"message": "TF400813: Resource not found."})),
    );
    let client = client_for(&server);
    let url = format!("{}/blob/missing.zip", server.base_url());

    let error = client.get_raw(&url).expect_err("the status is an error");

    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.status, Some(404));
    assert_eq!(error.message, NOT_FOUND_MESSAGE);
    let details = error.details.expect("details carry the status and body");
    assert_eq!(details["status"], json!(404));
    assert_eq!(
        details["body"],
        json!("{\"message\":\"TF400813: Resource not found.\"}")
    );
}
