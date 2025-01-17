use std::str::FromStr;
use std::sync::Arc;
use super::super::super::csrf;

#[derive(rocket::response::Responder)]
pub enum Responder {
    Ok(rocket::response::Redirect),
    NoAuth(crate::rocket::auth::NoAuth),
    CsrdError(csrf::CsrfTokenError),
    Err((rocket::http::Status, std::borrow::Cow<'static, str>)),
}

#[rocket::get("/twitch/oauth?<code>&<scope>&<state>", rank=0)]
pub async fn oauth_ok(code: &str, scope: &str, state: &str, csrf: Result<csrf::CsrfToken<csrf::State>, csrf::CsrfTokenError>, auth: Result<&crate::rocket::auth::Auth, crate::rocket::auth::NoAuth>, cookie_jar: &rocket::http::CookieJar<'_>) -> Responder {
    let _ = scope;
    let auth = match auth {
        Err(err) => return Responder::NoAuth(err),
        Ok(auth) => auth,
    };
    match csrf {
        Err(err) => return Responder::CsrdError(err),
        Ok(_) => { },
    }

    let url = match url::Url::from_str(crate::rocket::auth::twitch::OAUTH_URL) {
        Ok(v) => v,
        Err(err) => return Responder::Err((rocket::http::Status::InternalServerError, err.to_string().into())),
    };
    let mut builder = twitch_api::twitch_oauth2::UserToken::builder(
        auth.twitch.client_id.clone(),
        auth.twitch.client_secret.clone(),
        url
    );
    builder.set_csrf(twitch_api::twitch_oauth2::CsrfTokenRef::from_str(state).to_owned());
    match builder.get_user_token(
        &auth.twitch.client,
        state,
        code
    ).await {
        Ok(token) => {
            match super::session::SessionCookie::from(&token).as_cookie() {
                Ok(cookie) => cookie_jar.add_private(cookie),
                Err(err) => {
                    tracing::warn!("Failed to create cookie from UserToken: {err}");
                    return Responder::Err((rocket::http::Status::InternalServerError, "Cannot create cookie from UserToken".into()))
                },
            }
            auth.twitch.auth.authentications.upsert_async(token.user_id.clone(), From::from(token.clone())).await;
            Responder::Ok(rocket::response::Redirect::to("/twitch"))
        },
        Err(err) => {
            Responder::Err((rocket::http::Status::InternalServerError, err.to_string().into()))
        }
    }
}