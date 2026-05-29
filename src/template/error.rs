#[derive(askama::Template, Debug)]
#[template(path = "error.html")]
pub struct Error<'a> {
    pub error: &'a str,
    pub error_description: &'a str,
}