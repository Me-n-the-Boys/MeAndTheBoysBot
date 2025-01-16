use std::sync::Arc;
use rocket::Request;
use rocket::request::Outcome;

pub(in crate::rocket) trait QueryParameter{
    const PARAMETER: &'static str;
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct State;
impl QueryParameter for State{
    const PARAMETER: &'static str = "state";
}
#[derive(rocket::form::FromForm, Debug)]
pub(in crate::rocket) struct CsrfToken<P:QueryParameter>{
    pub token: Vec<u8>,
    _parameter: std::marker::PhantomData<P>,
}

#[derive(rocket::response::Responder, Debug)]
pub enum CsrfTokenError {
    Err(rocket::response::content::RawHtml<std::borrow::Cow<'static,str>>),
}

#[rocket::async_trait]
impl<'a, P:QueryParameter> rocket::request::FromRequest<'a> for CsrfToken<P> {
    type Error = CsrfTokenError;

    async fn from_request(request: &'a Request<'_>) -> Outcome<Self, Self::Error> {
        let token = match request.query_value::<super::base64::Base64Decode>(P::PARAMETER) {
            Some(token) => token,
            None => return Outcome::Error((rocket::http::Status::Unauthorized, CsrfTokenError::Err(rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>No CSRF Token</title>
    </head>
    <body>
        <h1>No CSRF Token</h1>
        <p>This request expected a CSRF Token in the <code>{}</code> parameter, but none was found</p>
    </body>
</html>"#, P::PARAMETER).into())))),
        };

        let token = match token {
            Ok(token) => token,
            Err(err) => return Outcome::Error((rocket::http::Status::BadRequest, CsrfTokenError::Err(rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>Error Parsing CSRF Token</title>
    </head>
    <body>
        <h1>Error Parsing CSRF Token</h1>
        <p>Error: <code>{err}</code></p>
    </body>
</html>
            "#).into())))),
        };

        let auth: &Arc<crate::rocket::auth::Auth> = match request.rocket().state() {
            Some(v) => v,
            None => return Outcome::Error((rocket::http::Status::InternalServerError, CsrfTokenError::Err(rocket::response::content::RawHtml(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>Internal Server Error</title>
    </head>
    <body>
        <h1>Internal Server Error</h1>
        <p>Invalid configuration: Twitch client not found</p>
    </body>
</html>
            "#.into())))),
        };

        match auth.csrf_tokens.get_async(token.data.as_slice()).await {
            Some(entry) => {
                entry.remove_entry();
                Outcome::Success(Self{
                    token: token.data,
                    _parameter: std::marker::PhantomData,
                })
            }
            None => {
                Outcome::Error((rocket::http::Status::Unauthorized, CsrfTokenError::Err(rocket::response::content::RawHtml(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>Invalid CSRF Token</title>
    </head>
    <body>
        <h1>Invalid CSRF Token</h1>
        <p>The provided CSRF Token is invalid</p>
    </body>
</html>
            "#.into()))))
            }
        }
    }
}