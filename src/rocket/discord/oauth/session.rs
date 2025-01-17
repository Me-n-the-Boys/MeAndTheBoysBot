use std::sync::Arc;
use rocket::{time, Request};
use rocket::http::private::cookie::Expiration;
use rocket::request::Outcome;
use crate::rocket::auth::NoAuth;

pub const SESSION_COOKIE: &str = "discord_session";

#[non_exhaustive]
pub struct Session {
    pub(crate) session: Vec<u8>,
    pub(crate) auth: crate::rocket::auth::discord::token::Token,
    pub(crate) current_user: serenity::model::user::CurrentUser,
}

#[derive(rocket::response::Responder, Debug)]
pub enum Responder {
    NoAuth(NoAuth),
    Error(&'static str),
}

#[rocket::async_trait]
impl<'a> rocket::request::FromRequest<'a> for Session {
    type Error = Responder;

    async fn from_request(request: &'a Request<'_>) -> Outcome<Self, Self::Error> {
        let mut cookie = match request.cookies().get_private(SESSION_COOKIE) {
            None => return Outcome::Forward(rocket::http::Status::Unauthorized),
            Some(v) => v,
        };
        if let Some(v) = time::OffsetDateTime::now_utc().checked_add(time::Duration::days(7)) {
            cookie.set_expires(Expiration::DateTime(v));
        }
        cookie.set_secure(Some(true));
        use base64::engine::Engine;

        let session = match base64::engine::general_purpose::URL_SAFE.decode(cookie.value().as_bytes()) {
            Ok(v) => v,
            Err(_) => {
                request.cookies().remove(SESSION_COOKIE);
                return Outcome::Forward(rocket::http::Status::Unauthorized);
            },
        };
        request.cookies().add_private(cookie);
        let twitch: &Arc<crate::rocket::auth::Auth> = match request.rocket().state() {
            Some(v) => v,
            None => return Outcome::Error((rocket::http::Status::InternalServerError, Responder::NoAuth(NoAuth))),
        };
        let mut auth = match twitch.discord.auth.tokens.get_async(session.as_slice()).await {
            None => return Outcome::Forward(rocket::http::Status::Unauthorized),
            Some(v) => v,
        };
        let mut http = serenity::http::Http::new(format!("Bearer {}", auth.access_token).as_str());
        let current_user = match http.get_current_user().await {
            Ok(v) => v,
            Err(err) => {
                let mut err = ::anyhow::format_err!("Failed to get current user: {err}");
                match crate::rocket::auth::discord::token::TokenRequest::refresh_token(&auth.refresh_token).request_token(&twitch.discord).await {
                    Ok(v) => {
                        *auth = v;
                        http = serenity::http::Http::new(format!("Bearer {}", auth.access_token).as_str());
                        match http.get_current_user().await {
                            Ok(v) => v,
                            Err(err2) => {
                                err = err.context(::anyhow::format_err!("Failed to get current user twice. One before refreshing the token and once after: {err2}"));
                                tracing::warn!("Failed to get current user twice. One before refreshing the token and once after: {err}");
                                return Outcome::Error((rocket::http::Status::BadRequest, Responder::Error("Failed to get current user twice. One before refreshing the token and once after")));
                            }
                        }
                    }
                    Err(err2) => {
                        err = err.context(anyhow::format_err!("Failed to refresh token: {err2}"));
                        tracing::warn!("Failed to get current user and failed to refresh token: {err}");
                        return Outcome::Error((rocket::http::Status::BadRequest, Responder::Error("Failed to get current user and Failed to refresh token")));
                    }
                }
            },
        };
        let auth = auth.clone();
        Outcome::Success(Self{
            session,
            auth,
            current_user,
        })
    }
}