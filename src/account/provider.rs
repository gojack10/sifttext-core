//! OAuth endpoints stay within the selected trusted account service or realm.
use super::{PLATFORM, http_response, json_response, text};
use crate::Result;

pub(super) const PRODUCTION_ISSUER: &str = "https://auth.sifttext.com/realms/sifttext";
const PRODUCTION_CLIENT: &str = "sifttext-core";

pub(super) struct Provider {
    pub issuer: String,
    pub authorization: String,
    pub token: String,
    pub revocation: String,
    pub registration: Option<String>,
    pub client_id: Option<String>,
    pub verify_callback_issuer: bool,
}

pub(super) fn configured_issuer(origin: &str) -> Result<Option<String>> {
    let Some(value) = std::env::var_os("SIFTTEXT_AUTH_ISSUER") else {
        return Ok(None);
    };
    let value = value.into_string().map_err(|_| "invalid issuer encoding")?;
    if origin == PLATFORM {
        if value != PRODUCTION_ISSUER {
            return Err("production account origin requires its exact production issuer".into());
        }
        return Ok(Some(value));
    }
    if origin == "https://staging.sifttext.com" {
        if value != "https://auth-staging.sifttext.com/realms/sifttext" {
            return Err("staging account origin requires its exact staging issuer".into());
        }
        return Ok(Some(value));
    }
    let (port, path) = value
        .strip_prefix("http://127.0.0.1:")
        .and_then(|s| s.split_once('/'))
        .ok_or("issuer must be a loopback URL with a realm path")?;
    if !origin.starts_with("http://127.0.0.1:")
        || !port.parse::<u16>().is_ok_and(|p| p > 0)
        || path.is_empty()
        || path.ends_with('/')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
    {
        return Err("invalid loopback issuer".into());
    }
    Ok(Some(value))
}

pub(super) fn for_login(origin: &str) -> Result<Provider> {
    if let Some(issuer) = configured_issuer(origin)? {
        return for_issuer(origin, &issuer);
    }
    // The legacy authority has no Keycloak realm. Only an absent realm
    // selects legacy login; malformed discovery and outages fail closed.
    if origin == PLATFORM
        && let Some(provider) = discover(PRODUCTION_ISSUER)?
    {
        return Ok(provider);
    }
    Ok(legacy(origin))
}

pub(super) fn for_issuer(origin: &str, issuer: &str) -> Result<Provider> {
    if issuer.is_empty() {
        return Ok(legacy(origin));
    }
    discover(issuer)?.ok_or_else(|| "Saved sign-in authority is unavailable. Run sifttext login.".into())
}

fn legacy(origin: &str) -> Provider {
    Provider {
        issuer: String::new(),
        authorization: format!("{origin}/oauth/authorize"),
        token: format!("{origin}/oauth/token"),
        revocation: format!("{origin}/oauth/revoke"),
        registration: Some(format!("{origin}/oauth/register")),
        client_id: None,
        verify_callback_issuer: false,
    }
}

fn discover(issuer: &str) -> Result<Option<Provider>> {
    let (status, body) = http_response(
        issuer, "/.well-known/openid-configuration", None, None, "GET", None,
    )?;
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        return Err(format!("OIDC discovery returned HTTP {status}").into());
    }
    let document = json_response(status, &body)?;
    if text(&document, "issuer")? != issuer {
        return Err("OIDC discovery did not confirm the configured issuer".into());
    }
    if !document["code_challenge_methods_supported"]
        .as_array()
        .is_some_and(|methods| methods.iter().any(|method| method == "S256"))
    {
        return Err("OIDC provider does not advertise PKCE S256".into());
    }
    let endpoint = |field: &str| -> Result<String> {
        let value = text(&document, field)?;
        let suffix = value
            .strip_prefix(&format!("{issuer}/"))
            .ok_or("discovered endpoint is outside the configured issuer")?;
        if suffix.is_empty()
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
        {
            return Err("unsupported discovered endpoint path".into());
        }
        Ok(value.to_owned())
    };
    let authorization = endpoint("authorization_endpoint")?;
    let token = endpoint("token_endpoint")?;
    let revocation = endpoint("revocation_endpoint")?;
    let client_id = if issuer == PRODUCTION_ISSUER {
        PRODUCTION_CLIENT.to_owned()
    } else {
        std::env::var("SIFTTEXT_AUTH_CLIENT_ID")
            .map_err(|_| "set the pre-registered public OIDC client ID")?
    };
    if client_id.is_empty() || client_id.len() > 256 || client_id.chars().any(char::is_control) {
        return Err("invalid OIDC client ID".into());
    }
    let verify_callback_issuer = document["authorization_response_iss_parameter_supported"] == true;
    if issuer == PRODUCTION_ISSUER && !verify_callback_issuer {
        return Err("production OIDC must include the issuer in its authorization response".into());
    }
    Ok(Some(Provider {
        issuer: issuer.to_owned(),
        authorization,
        token,
        revocation,
        registration: None,
        client_id: Some(client_id),
        verify_callback_issuer,
    }))
}
