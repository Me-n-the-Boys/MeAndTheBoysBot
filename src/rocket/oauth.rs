use std::str::FromStr;
use crate::twitch_client::UserTokenBuilder;

async fn validate_state<'a>(state: &'a str, twitch: &rocket::State<crate::twitch_client::Twitch>) -> Result<Vec<u8>, impl rocket::response::Responder<'a, 'a>> {
    use base64::engine::Engine;
    match base64::engine::general_purpose::GeneralPurpose::decode(&base64::engine::general_purpose::URL_SAFE, state.as_bytes()) {
        Ok(state) => if twitch.inner().csrf_tokens.remove_async(state.as_slice()).await {
            Err((rocket::http::Status::BadRequest, r#"
<html lang="en">
<head>
    <title>Invalid CSRF Code</title>
</head>
<body>
    <h1>Invalid CSRF Code</h1>
    <p>The CSRF code has already been used, or it has expired. Please try Authenticating again <a href="./new_oauth">here</a>.</p>
</body>
</html>
"#))
        } else {
            Ok(state)
        },
        Err(_) => Err((rocket::http::Status::BadRequest, r#"
<html lang="en">
<head>
    <title>Invalid CSRF Code</title>
</head>
<body>
    <h1>Invalid CSRF Code</h1>
    <p>The CSRF code has already been used, or it has expired. Please try Authenticating again <a href="./new_oauth">here</a>.</p>
</body>
</html>

Invalid State (Not Base64 encoded)"#)),
    }
}

#[rocket::get("/twitch/new_oauth")]
pub(in super) async fn new_oauth<'r>(twitch: &rocket::State<crate::twitch_client::Twitch>) -> Result<rocket::response::Redirect, impl rocket::response::Responder<'r, 'static>> {
    match twitch.get_new_oath().await {
        Ok(url) => Ok(rocket::response::Redirect::temporary(url)),
        Err(err) => Err((rocket::http::Status::InternalServerError, err)),
    }
}

#[rocket::get("/twitch/oauth?<code>&<scope>&<state>")]
pub(in super) async fn oauth_ok<'r>(code: &'r str, scope: &'r str, state: &'r str, twitch: &rocket::State<crate::twitch_client::Twitch>, user_token_builder: UserTokenBuilder)
    -> Result<
        (rocket::http::Status, rocket::response::content::RawHtml<std::borrow::Cow<'static, str>>),
        Result<
            (rocket::http::Status, std::borrow::Cow<'static, str>),
            impl rocket::response::Responder<'r, 'r>
        >
    > {
    let _ = scope;
    match validate_state(state, twitch).await {
        Err(err) => return Err(Err(err)),
        Ok(_) => {},
    };

    let url = match url::Url::from_str(user_token_builder.url.as_str()) {
        Ok(v) => v,
        Err(err) => return Err(Ok((rocket::http::Status::InternalServerError, err.to_string().into()))),
    };
    let mut builder = twitch_api::twitch_oauth2::UserToken::builder(
        user_token_builder.client_id.clone(),
        user_token_builder.client_secret.clone(),
        url
    );
    builder.set_csrf(twitch_api::twitch_oauth2::CsrfTokenRef::from_str(state).to_owned());
    match builder.get_user_token(
        &twitch.inner().client,
        state,
        code
    ).await {
        Ok(token) => {
            twitch.inner().auth.authentications.upsert_async(token.user_id.clone(), From::from(token.clone())).await;
            Ok((rocket::http::Status::Ok, rocket::response::content::RawHtml(format!(r#"
<html lang="en">
<head>
    <title>OAuth Success</title>
</head>
<body>
    <h1>OAuth Success</h1>
    <p>Authenticated with the username "{username}" and user_id "{user_id}".</p>
</body>
</html>
            "#, username=token.login, user_id=token.user_id).into())))
        },
        Err(err) => {
            Err(Ok((rocket::http::Status::InternalServerError, err.to_string().into())))
        }
    }
}

#[rocket::get("/twitch/oauth?<error>&<error_description>&<state>")]
pub(in super) async fn oauth_err<'r>(error: &'r str, error_description: &'r str, state: &'r str, twitch: &rocket::State<crate::twitch_client::Twitch>) -> Result<(rocket::http::Status, rocket::response::content::RawHtml<String>),impl rocket::response::Responder<'r, 'r>> {
    if let Err(err) = validate_state(state, twitch).await {
        return Err(err);
    }
    Ok((rocket::http::Status::Ok, rocket::response::content::RawHtml(format!(r#"
<html lang="en">
    <head>
        <title>OAuth Error</title>
    </head>
    <body>
        <h1>OAuth Error</h1>
        <p>Error: <code>{error}</code></p>
        <p>Description: <code>{error_description}</code></p>
    </body>
</html>
"#))))
}
