//! Request headers for a resolved credential.

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::config::AuthMethod;
use crate::credentials::Credentials;

/// PATs are sent as HTTP Basic with an empty username — `Basic base64(":PAT")` —
/// and OAuth tokens as Bearer, matching the Elixir CLI.
pub fn auth_header(credentials: &Credentials) -> (String, String) {
    let value = match credentials.method {
        AuthMethod::Pat => format!(
            "Basic {}",
            STANDARD.encode(format!(":{}", credentials.token))
        ),
        AuthMethod::Device | AuthMethod::Browser => format!("Bearer {}", credentials.token),
    };

    ("Authorization".to_owned(), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials(method: AuthMethod, token: &str) -> Credentials {
        Credentials {
            org: "myorg".to_owned(),
            method,
            token: token.to_owned(),
        }
    }

    #[test]
    fn pat_header_is_basic_with_colon_prefix() {
        let (name, value) = auth_header(&credentials(AuthMethod::Pat, "abc"));

        assert_eq!(name, "Authorization");
        assert_eq!(value, "Basic OmFiYw==");
        assert!(!value.contains("abc"), "the token must not appear raw");
    }

    #[test]
    fn oauth_tokens_are_sent_as_bearer() {
        for method in [AuthMethod::Device, AuthMethod::Browser] {
            let (name, value) = auth_header(&credentials(method, "oauth-token"));

            assert_eq!(name, "Authorization");
            assert_eq!(value, "Bearer oauth-token");
        }
    }
}
