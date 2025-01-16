use base64::Engine;
use super::super::super::csrf;

#[derive(rocket::response::Responder)]
pub enum Responder {
    Ok(rocket::response::Redirect),
    HtmlErr((rocket::http::Status, rocket::response::content::RawHtml<std::borrow::Cow<'static, str>>)),
}

#[rocket::get("/discord/oauth?<code>&<state>", rank=0)]
pub async fn oauth_ok(code: &str, state: &str, _csrf: csrf::CsrfToken<csrf::State>, auth: &rocket::State<crate::rocket::auth::Auth>, cookie_jar: &rocket::http::CookieJar<'_>) -> Responder {
    let token = match crate::rocket::auth::discord::token::TokenRequest::authorization_code(code, state)
        .request_token(&auth.discord).await {
        Ok(v) =>v,
        Err(err) => {
            tracing::warn!("Failed to get token: {err}");
            let err = err.root_cause().to_string();
            return Responder::HtmlErr((rocket::http::Status::UnprocessableEntity, rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>OAuth Error</title>
    </head>
    <body>
        <h1>Error Getting OAuth Token</h1>
        <p>Error: <code>{err}</code></p>
    </body>
"#).into())))
        }
    };
    let key = auth.discord.auth.insert_new_token(token).await;
    let key = base64::engine::general_purpose::URL_SAFE.encode(&key);
    let mut cookie = rocket::http::Cookie::new(super::session::SESSION_COOKIE, key);
    cookie.set_secure(true);
    if let Some(v) = rocket::time::OffsetDateTime::now_utc().checked_add(rocket::time::Duration::days(7)) {
        cookie.set_expires(v);
    }
    cookie_jar.add(cookie);
    Responder::Ok(rocket::response::Redirect::to("/"))
}