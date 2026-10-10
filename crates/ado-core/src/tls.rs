//! The TLS trust anchors both `ureq` agents share.
//!
//! ureq's `rustls` feature alone verifies against `webpki-roots`: Mozilla's root
//! list, compiled into the binary, with the operating system's certificate store
//! never consulted. A TLS-inspecting proxy — Zscaler, or any other enterprise
//! appliance — signs the connection with a root that lives in that store and
//! nowhere else, so every request through it fails the handshake as an unknown
//! issuer. The frozen Elixir CLI worked behind one because OTP's `:public_key`
//! read the system store; `RootCerts::PlatformVerifier` is how the Rust build
//! keeps that behaviour.
//!
//! The verifier is `rustls-platform-verifier`. macOS and iOS verify through
//! `Security.framework` and Windows through its own certificate API, so both the
//! roots the operating system trusts and the ones its administrator explicitly
//! distrusts take effect. Linux and the BSDs read the system CA bundle
//! (`rustls-native-certs` over `openssl-probe`) and honour `SSL_CERT_FILE` and
//! `SSL_CERT_DIR`, which matters to the static `musl` archives that carry no
//! bundle of their own.

use ureq::tls::{RootCerts, TlsConfig};

/// The `TlsConfig` both agents build with: the platform's own trust decisions.
pub fn config() -> TlsConfig {
    TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_verifier_is_the_choice() {
        assert!(matches!(config().root_certs(), RootCerts::PlatformVerifier));
    }
}
