use super::super::super::csrf;

#[rocket::get("/twitch/oauth?<error>&<error_description>&<state>", rank=1)]
pub async fn oauth_err<'r>(error: &'r str, error_description: &'r str, state: &'r str, csrf: Result<csrf::CsrfToken<csrf::State>, csrf::CsrfTokenError>) -> Result<rocket::response::content::RawHtml<String>, csrf::CsrfTokenError> {
    let _ = csrf?;

    let _ = state;
    Ok(rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>OAuth Error</title>
    </head>
    <body>
        <h1>OAuth Error</h1>
        <p>Error: <code>{error}</code></p>
        <p>Description: <code>{error_description}</code></p>
    </body>
</html>
"#)))
}