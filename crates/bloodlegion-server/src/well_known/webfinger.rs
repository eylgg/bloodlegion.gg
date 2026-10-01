use anyhow::Context;
use axum::{
    Json,
    extract::Query,
    http::{StatusCode, header},
    response::IntoResponse,
};
use thiserror::Error;
use url::Url;

use crate::{Problem, Result, State};

const OIDC_ISSUER_REL: &str = "http://openid.net/specs/connect/1.0/issuer";

#[derive(Debug, serde::Deserialize)]
pub struct Payload {
    rel: Option<String>,
    resource: String,
}

#[derive(Debug, serde::Serialize)]
struct ResourceDescriptor {
    subject: String,
    links: Vec<Link>,
}

#[derive(Debug, serde::Serialize)]
struct Link {
    rel: String,
    href: String,
}

#[derive(Debug, Error, Problem)]
pub enum WebfingerError {
    #[error("unsupported relation")]
    #[problem(
        status = NOT_FOUND,
        title = "Not Found",
        detail = "This server does not support the requested relation type."
    )]
    UnsupportedRelation,
    #[error("missing acct prefix")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "You must begin the resource parameter with the 'acct:' prefix."
    )]
    MissingAcctPrefix,
    #[error("invalid resource format")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "You must include an '@' symbol to separate the username and the domain."
    )]
    InvalidResourceFormat,
    #[error("resource not found")]
    #[problem(
        status = NOT_FOUND,
        title = "Not Found",
        detail = "This server does not serve the requested resource."
    )]
    ResourceNotFound,
}

pub async fn handler(
    state: State,
    Query(payload): Query<Payload>,
) -> Result<impl IntoResponse, WebfingerError> {
    // An empty `rel` means "return all supported links"; we support only one, so
    // a specified `rel` that isn't ours is rejected.
    if payload
        .rel
        .as_deref()
        .is_some_and(|rel| rel != OIDC_ISSUER_REL)
    {
        return Err(crate::Error::External(WebfingerError::UnsupportedRelation));
    }

    let acct = payload
        .resource
        .strip_prefix("acct:")
        .ok_or(crate::Error::External(WebfingerError::MissingAcctPrefix))?;

    let (_username, domain) = acct.split_once('@').ok_or(crate::Error::External(
        WebfingerError::InvalidResourceFormat,
    ))?;

    let url = Url::parse(&state.origin).context("parsing the configured origin")?;
    if url.host_str() != Some(domain) {
        return Err(crate::Error::External(WebfingerError::ResourceNotFound));
    }

    let resource_descriptor = ResourceDescriptor {
        subject: payload.resource,
        links: vec![Link {
            rel: OIDC_ISSUER_REL.to_string(),
            href: state.origin.to_string(),
        }],
    };

    let headers = [
        (header::CONTENT_TYPE, "application/jrd+json"),
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
    ];

    Ok((StatusCode::OK, headers, Json(resource_descriptor)).into_response())
}
