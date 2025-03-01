use std::sync::Arc;
use rocket::{time, Request};
use rocket::request::Outcome;
use crate::rocket::auth::NoAuth;

const SESSION_COOKIE: &str = "twitch_session";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "v")]
pub enum SessionCookie{
    V1{
        user_id: twitch_api::types::UserId,
    }
}

impl<'a> From<&'a twitch_api::twitch_oauth2::UserToken> for SessionCookie {
    fn from(token: &'a twitch_api::twitch_oauth2::UserToken) -> Self {
        Self::V1 {
            user_id: token.user_id.clone(),
        }
    }
}
impl SessionCookie {
    pub fn as_cookie(&self) -> Result<rocket::http::Cookie<'static>, serde_json::Error> {
        let mut cookie = rocket::http::Cookie::new(SESSION_COOKIE, serde_json::to_string(&self)?);
        if let Some(v) = time::OffsetDateTime::now_utc().checked_add(time::Duration::days(7)) {
            cookie.set_expires(v);
        }
        cookie.set_http_only(true);
        cookie.set_secure(true);
        cookie.set_same_site(rocket::http::SameSite::Strict);
        Ok(cookie)
    }
}

#[non_exhaustive]
pub struct Session {
    pub(crate) session: SessionCookie,
    pub(crate) auth: crate::twitch_client::TwitchAuthentication,
}
#[rocket::async_trait]
impl<'a> rocket::request::FromRequest<'a> for Session {
    type Error = NoAuth;

    async fn from_request(request: &'a Request<'_>) -> Outcome<Self, Self::Error> {
        let mut cookie = match request.cookies().get_private(SESSION_COOKIE) {
            None => return Outcome::Forward(rocket::http::Status::Unauthorized),
            Some(v) => v,
        };
        if let Some(v) = time::OffsetDateTime::now_utc().checked_add(time::Duration::days(7)) {
            cookie.set_expires(v);
        }
        cookie.set_secure(Some(true));
        let session = match serde_json::from_str::<SessionCookie>(cookie.value()) {
            Ok(v) => v,
            Err(_) => {
                request.cookies().remove(SESSION_COOKIE);
                return Outcome::Forward(rocket::http::Status::Unauthorized);
            },
        };
        request.cookies().add_private(cookie);
        let auth: &Arc<crate::rocket::auth::Auth> = match request.rocket().state() {
            Some(v) => v,
            None => return Outcome::Error((rocket::http::Status::InternalServerError, NoAuth)),
        };
        let mut token = match &session {
            SessionCookie::V1 { user_id } => {
                match auth.twitch.auth.authentications.get_async(user_id).await {
                    None => return Outcome::Forward(rocket::http::Status::Unauthorized),
                    Some(v) => v,
                }
            }
        };
        if !token.check_valid(&auth.twitch).await {
            auth.twitch.auth.authentications.remove_async(&token.user_id).await;
            return Outcome::Forward(rocket::http::Status::Unauthorized);
        }
        let auth = token.clone();
        Outcome::Success(Self{
            session,
            auth,
        })
    }
}