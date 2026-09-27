//! R34: the one test that leaves the machine. It proves the rustls handshake,
//! the bundled roots (ureq's `rustls` feature links `webpki-roots` v1.0.9, not
//! the operating system's certificate store) and the redirect-to-`auth_required`
//! mapping together, against `https://dev.azure.com`.
//!
//! Deliberately `#[ignore]`d: neither `just ci` nor a GitHub runner may depend on
//! the network. Run it by hand with
//! `cargo test -p ado-core --test tls_smoke -- --ignored --nocapture`.

use ado_core::client::Client;
use ado_core::config::AuthMethod;
use ado_core::credentials::Credentials;
use ado_core::env::{ENV_SERVER, MapEnv};
use ado_core::error::ErrorCode;

/// The org-less root: an unauthenticated request is answered with a redirect to
/// the sign-in page, which spec §6.2 classifies as `auth_required`.
const ORG_LESS_ROOT: &str = "https://dev.azure.com";

#[test]
#[ignore = "leaves the machine; run with `cargo test -p ado-core --test tls_smoke -- --ignored`"]
fn an_unauthenticated_get_reaches_dev_azure_devops_and_maps_to_auth_required() {
    let env = MapEnv::new().set(ENV_SERVER, ORG_LESS_ROOT);
    let client = Client::from_env(&no_credentials(), &env).expect("a client");
    let url = client.url_for("", &[]);

    let error = client
        .get("", &[])
        .expect_err("an unauthenticated GET cannot succeed");

    eprintln!(
        "tls smoke: GET {url} -> {} (status {:?}): {}",
        error.code.as_str(),
        error.status,
        error.message
    );

    assert_eq!(
        error.code,
        ErrorCode::AuthRequired,
        "GET {url} produced {error:?}"
    );
}

/// No token material: an empty organization and an empty PAT, which is what an
/// unconfigured machine resolves to before `ado login` (the header is an empty
/// Basic, `Basic Og==`).
fn no_credentials() -> Credentials {
    Credentials {
        org: String::new(),
        method: AuthMethod::Pat,
        token: String::new(),
    }
}
