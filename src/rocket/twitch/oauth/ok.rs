use std::str::FromStr;
use std::sync::Arc;
use super::super::super::csrf;

#[derive(rocket::response::Responder)]
pub enum Responder {
    Ok(rocket::response::Redirect),
    Err((rocket::http::Status, std::borrow::Cow<'static, str>)),
}

#[rocket::get("/twitch/oauth?<code>&<scope>&<state>", rank=0)]
pub async fn oauth_ok(code: &str, scope: &str, state: &str, _csrf: csrf::CsrfToken<csrf::State>, twitch: &rocket::State<Arc<crate::twitch_client::Twitch>>, cookie_jar: &rocket::http::CookieJar<'_>) -> Responder {
    let _ = scope;

    let url = match url::Url::from_str(twitch.oauth_url.as_str()) {
        Ok(v) => v,
        Err(err) => return Responder::Err((rocket::http::Status::InternalServerError, err.to_string().into())),
    };
    let mut builder = twitch_api::twitch_oauth2::UserToken::builder(
        twitch.client_id.clone(),
        twitch.client_secret.clone(),
        url
    );
    builder.set_csrf(twitch_api::twitch_oauth2::CsrfTokenRef::from_str(state).to_owned());
    match builder.get_user_token(
        &twitch.inner().client,
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
            twitch.inner().auth.authentications.upsert_async(token.user_id.clone(), From::from(token.clone())).await;
            Responder::Ok(rocket::response::Redirect::to("/twitch"))
        },
        Err(err) => {
            Responder::Err((rocket::http::Status::InternalServerError, err.to_string().into()))
        }
    }
}