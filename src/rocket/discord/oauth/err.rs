use crate::rocket::csrf;

#[rocket::get("/discord/oauth?<error>&<error_description>&<state>", rank=1)]
pub async fn oauth_err<'r>(error: &'r str, error_description: Option<&'r str>, state: &'r str, _csrf: csrf::CsrfToken<csrf::State>) -> rocket::response::content::RawHtml<String> {
    let _ = state;
    let error_description = error_description.unwrap_or("");
    rocket::response::content::RawHtml(format!(r#"
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
"#))
}