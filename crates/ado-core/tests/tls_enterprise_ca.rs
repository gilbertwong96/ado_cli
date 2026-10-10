//! The enterprise-CA reproduction: the one test that shows the trust-anchor choice is
//! what makes a TLS-inspecting proxy work.
//!
//! Such a proxy signs with a root that is in the operating system's store and in no
//! bundled list. This test stands a TLS server up on `127.0.0.1` with a certificate from
//! exactly that kind of CA and asserts both halves of the choice:
//! [`ado_core::tls::config`] — the platform verifier both agents ship with — completes
//! the handshake, so the request reaches the server, while the same request on a
//! `RootCerts::WebPki` agent (the bundled Mozilla list every build before it used)
//! fails verification and never arrives.
//!
//! Deliberately `#[ignore]`d: it needs a CA the platform trusts, which is the machine's
//! own business and not a runner's. On Linux, either install `ca.pem` in the system
//! bundle or point `SSL_CERT_FILE` at it; on macOS, `security add-trusted-cert -r
//! trustRoot -p ssl -k ~/Library/Keychains/login.keychain-db ca.pem`, which asks for your
//! password. A bare `security import` is not enough on macOS: the CA needs that explicit
//! trust setting as well, which is what a proxy's own installer does for you. On a
//! machine you would rather not touch, a Linux container with `SSL_CERT_FILE` set is the
//! same proof with no system state changed. Then:
//!
//! ```text
//! ADO_TEST_TLS_CERT=localhost.pem ADO_TEST_TLS_KEY=localhost.key \
//!   cargo test -p ado-core --test tls_enterprise_ca -- --ignored --nocapture
//! ```
//!
//! The throwaway CA and the leaf are one `openssl` recipe:
//!
//! ```text
//! openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.pem -days 1 \
//!   -subj '/CN=ado enterprise CA test' -addext 'basicConstraints=critical,CA:TRUE'
//! openssl req -newkey rsa:2048 -nodes -keyout localhost.key -out localhost.csr \
//!   -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost'
//! openssl x509 -req -in localhost.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
//!   -out localhost.crt -days 1 -copy_extensions copy
//! cat localhost.crt ca.pem > localhost.pem
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use ado_core::client::Client;
use ado_core::config::AuthMethod;
use ado_core::credentials::Credentials;
use ado_core::env::{ENV_SERVER, MapEnv};
use ado_core::error::ErrorCode;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::json;
use ureq::tls::{RootCerts, TlsConfig};

const CERT: &str = "ADO_TEST_TLS_CERT";
const KEY: &str = "ADO_TEST_TLS_KEY";

/// Two connections: the platform verifier's completed request, and the bundled list's
/// handshake, which the client abandons.
const CONNECTIONS: usize = 2;

#[test]
#[ignore = "needs a CA the platform trusts, plus ADO_TEST_TLS_CERT and ADO_TEST_TLS_KEY"]
fn a_ca_outside_the_bundled_roots_is_trusted_where_they_are_not() {
    let (port, server) = serve();

    let error = client_for(port)
        .get("", &[])
        .expect_err("the server answers 404, which is an error");
    assert_eq!(
        (error.code, error.status),
        (ErrorCode::NotFound, Some(404)),
        "the platform verifier never reached the server: {error:?}"
    );

    let error = bundled_roots_agent()
        .get(&format!("https://localhost:{port}/"))
        .call()
        .expect_err("the bundled list cannot verify the enterprise CA");
    eprintln!("bundled roots against the same server: {error}");

    server.join().expect("the server thread");
}

fn client_for(port: u16) -> Client {
    let env = MapEnv::new().set(ENV_SERVER, &format!("https://localhost:{port}"));
    let credentials = Credentials {
        org: "myorg".to_owned(),
        method: AuthMethod::Pat,
        token: "pat".to_owned(),
    };

    Client::from_env(&credentials, &env).expect("a client")
}

/// The configuration the build used before [`ado_core::tls`]: Mozilla's list, compiled
/// in, with the operating system's store never consulted.
fn bundled_roots_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().root_certs(RootCerts::WebPki).build())
        .build()
        .new_agent()
}

/// A TLS server on `127.0.0.1` that answers `CONNECTIONS` connections and skips the ones
/// whose handshake never completes.
fn serve() -> (u16, JoinHandle<()>) {
    let certificates = CertificateDer::pem_file_iter(path(CERT))
        .expect("a readable certificate file")
        .collect::<Result<Vec<_>, _>>()
        .expect("PEM certificates");
    let key = PrivateKeyDer::from_pem_file(path(KEY)).expect("a readable private key file");
    let config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .expect("a server config"),
    );

    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let port = listener.local_addr().expect("the bound address").port();

    let server = thread::spawn(move || {
        for connection in listener.incoming().take(CONNECTIONS) {
            let Ok(stream) = connection else { break };
            let Ok(connection) = ServerConnection::new(config.clone()) else {
                break;
            };
            let mut tls = StreamOwned::new(connection, stream);

            if read_request(&mut tls) {
                let body = json!({ "message": "no such thing" }).to_string();
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );

                let _ = tls.write_all(response.as_bytes());
                let _ = tls.flush();
            }
        }
    });

    (port, server)
}

/// `true` once the request's header block has arrived, `false` when the handshake or the
/// read fails — which is what a client that refuses the certificate produces.
fn read_request(tls: &mut StreamOwned<ServerConnection, TcpStream>) -> bool {
    let mut reader = BufReader::new(tls);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return false,
            Ok(_) if line == "\r\n" => return true,
            Ok(_) => {}
        }
    }
}

fn path(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must name a PEM file"))
}
