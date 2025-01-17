use std::sync::Arc;
use rocket::http::uri::fmt::ValidRoutePrefix;

const BASE_URI: rocket::http::uri::Absolute<'static> = rocket::uri!("https://discord.com/oauth2/authorize?");
const SCOPES: &'static str = "identify connections guilds guilds.members.read";
#[rocket::get("/discord/new_oauth")]
pub async fn new_oauth<'r>(auth: &rocket::State<Arc<crate::rocket::auth::Auth>>) -> rocket::response::Redirect {
    let token = auth.get_new_csrf().await;
    let uri = BASE_URI.append("/".into(), Some(format!("response_type=code&prompt=none&scope={SCOPES}&redirect_uri={}&client_id={}&state={token}", crate::rocket::auth::discord::OAUTH_URL, auth.twitch.client_id).into()));
    rocket::response::Redirect::temporary(uri)
}
