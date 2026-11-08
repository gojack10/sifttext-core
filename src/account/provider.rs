//! Explicit staging/local OIDC configuration; production keeps its legacy route.
//! Discovery endpoints remain confined to the selected trusted issuer.
use super::{http, origin, text};
use crate::Result;

pub(super) struct Provider {
    pub issuer: String,
    pub authorization: String,
    pub token: String,
    pub revocation: String,
    pub registration: Option<String>,
    pub client_id: Option<String>,
    pub verify_callback_issuer: bool,
}

pub(super) fn issuer() -> Result<String> {
    let Some(value) = std::env::var_os("SIFTTEXT_AUTH_ISSUER") else {
        return Ok(String::new());
    };
    let value = value.into_string().map_err(|_| "invalid issuer encoding")?;
    if origin()? == "https://staging.sifttext.com" {
        if value != "https://auth-staging.sifttext.com/realms/sifttext" {
            return Err("staging account origin requires its exact staging issuer".into());
        }
        return Ok(value);
    }
    // Never redirect production credentials to a development IdP.
    if !origin()?.starts_with("http://127.0.0.1:") {
        return Err("OIDC development issuer requires a loopback account origin".into());
    }
    let (port, path) = value.strip_prefix("http://127.0.0.1:")
        .and_then(|s| s.split_once('/')).ok_or("issuer must be a loopback URL with a realm path")?;
    if !port.parse::<u16>().is_ok_and(|p| p > 0)
        || path.is_empty() || path.ends_with('/')
        || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
    {
        return Err("invalid loopback issuer".into());
    }
    Ok(value)
}

pub(super) fn discover() -> Result<Provider> {
    let origin = origin()?;
    let issuer = issuer()?;
    if issuer.is_empty() {
        return Ok(Provider {
            issuer,
            authorization: format!("{origin}/oauth/authorize"),
            token: format!("{origin}/oauth/token"),
            revocation: format!("{origin}/oauth/revoke"),
            registration: Some(format!("{origin}/oauth/register")),
            client_id: None,
            verify_callback_issuer: false,
        });
    }
    let (status, document) = http(&issuer, "/.well-known/openid-configuration", None, None)?;
    if status != 200 || text(&document, "issuer")? != issuer {
        return Err("OIDC discovery did not confirm the configured issuer".into());
    }
    if !document["code_challenge_methods_supported"].as_array()
        .is_some_and(|methods| methods.iter().any(|method| method == "S256"))
    {
        return Err("OIDC provider does not advertise PKCE S256".into());
    }
    let endpoint = |field: &str| -> Result<String> {
        let value = text(&document, field)?;
        let suffix = value.strip_prefix(&format!("{issuer}/"))
            .ok_or("discovered endpoint is outside the configured issuer")?;
        if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b)) {
            return Err("unsupported discovered endpoint path".into());
        }
        Ok(value.to_owned())
    };
    let authorization = endpoint("authorization_endpoint")?;
    let token = endpoint("token_endpoint")?;
    let revocation = endpoint("revocation_endpoint")?;
    let client_id = std::env::var("SIFTTEXT_AUTH_CLIENT_ID")
        .map_err(|_| "set the pre-registered public OIDC client ID")?;
    if client_id.is_empty() || client_id.len() > 256 || client_id.chars().any(char::is_control) {
        return Err("invalid OIDC client ID".into());
    }
    let verify_callback_issuer = document["authorization_response_iss_parameter_supported"] == true;
    Ok(Provider { issuer, authorization, token, revocation, registration: None, client_id: Some(client_id), verify_callback_issuer })
}
