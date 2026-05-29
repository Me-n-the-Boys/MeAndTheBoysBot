use askama::Template;
use crate::rocket::csrf;

#[rocket::get("/discord/oauth?<error>&<error_description>&<state>", rank=1)]
pub async fn oauth_err<'r>(error: &'r str, error_description: Option<&'r str>, state: &'r str, csrf: Result<csrf::CsrfToken<csrf::State>, csrf::CsrfTokenError>) -> Result<rocket::response::content::RawHtml<String>, csrf::CsrfTokenError> {
    let _ = csrf?;
    let _ = state;
    let error_description = error_description.unwrap_or("");

    Ok(rocket::response::content::RawHtml(crate::template::error::Error{
        error,
        error_description,
    }.render().unwrap_or_else(|v|format!("{v:?}"))))
}