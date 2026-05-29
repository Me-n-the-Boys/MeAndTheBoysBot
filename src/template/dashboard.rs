#[derive(askama::Template, Debug)]
#[template(path = "dashboard.html")]
pub struct Dashboard<'a> {
    pub twitch_name: &'a twitch_api::types::UserName,
    pub twitch_id: &'a twitch_api::types::UserId,
    pub discord_name: &'a str,
}