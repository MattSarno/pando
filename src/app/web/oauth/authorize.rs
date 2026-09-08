use crate::app::AppState;
use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use time::{Duration, OffsetDateTime};

const AUTH_CODE_TTL_MINUTES: i64 = 10;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: String,
    code_challenge_method: String,
}

#[derive(Deserialize)]
pub struct AuthorizeSubmit {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    state: Option<String>,
    passphrase: String,
}

pub async fn authorize_form(Query(params): Query<AuthorizeQuery>) -> Response {
    if params.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "unsupported response_type").into_response();
    }

    if params.code_challenge_method != "S256" {
        return (StatusCode::BAD_REQUEST, "unsupported code_challenge_method").into_response();
    }

    Html(include_str!("../../../../static/authorize.html")).into_response()
}

pub async fn authorize_submit(
    State(app_state): State<AppState>,
    Form(body): Form<AuthorizeSubmit>,
) -> Response {
    let authorized = bool::from(
        body.passphrase
            .as_bytes()
            .ct_eq(app_state.admin_passphrase.as_bytes()),
    );
    if !authorized {
        let mut retry_params = vec![
            ("response_type", "code"),
            ("client_id", body.client_id.as_str()),
            ("redirect_uri", body.redirect_uri.as_str()),
            ("code_challenge", body.code_challenge.as_str()),
            ("code_challenge_method", body.code_challenge_method.as_str()),
            ("error", "incorrect passphrase"),
        ];
        if let Some(state) = body.state.as_deref() {
            retry_params.push(("state", state));
        }

        return Redirect::to(&append_query("/authorize", &retry_params)).into_response();
    }

    if body.code_challenge_method != "S256" {
        return (StatusCode::BAD_REQUEST, "unsupported code_challenge_method").into_response();
    }

    if body.code_challenge.is_empty() {
        return (StatusCode::BAD_REQUEST, "code_challenge must not be empty").into_response();
    }

    let client_exists =
        sqlx::query("SELECT 1 FROM oauth_clients WHERE client_id = $1 AND $2 = ANY(redirect_uris)")
            .bind(&body.client_id)
            .bind(&body.redirect_uri)
            .fetch_optional(&app_state.database_pool)
            .await;

    match client_exists {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                "no such client/redirect_uri exists",
            )
                .into_response();
        }
        Err(error) => {
            eprintln!("authorize_submit: failed to look up client: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let code = Alphanumeric.sample_string(&mut rand::rng(), 32);
    let expires_at = OffsetDateTime::now_utc() + Duration::minutes(AUTH_CODE_TTL_MINUTES);

    let insert_result = sqlx::query(
        "INSERT INTO oauth_auth_codes (code, client_id, redirect_uri, code_challenge, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&code)
    .bind(&body.client_id)
    .bind(&body.redirect_uri)
    .bind(&body.code_challenge)
    .bind(expires_at)
    .execute(&app_state.database_pool)
    .await;

    if let Err(error) = insert_result {
        eprintln!("authorize_submit: failed to insert auth code: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut success_params = vec![("code", code.as_str())];
    if let Some(state) = body.state.as_deref() {
        success_params.push(("state", state));
    }

    Redirect::to(&append_query(&body.redirect_uri, &success_params)).into_response()
}

fn append_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    let query = serializer.finish();

    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}{query}")
}
