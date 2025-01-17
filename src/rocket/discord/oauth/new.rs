use std::sync::Arc;
use rocket::http::uri::fmt::ValidRoutePrefix;

const BASE_URI: rocket::http::uri::Absolute<'static> = rocket::uri!("https://discord.com/oauth2/authorize?");
const SCOPES: &'static str = "identify connections guilds guilds.members.read";
#[rocket::get("/discord/new_oauth")]
pub async fn new_oauth<'r>(auth: &rocket::State<Arc<crate::rocket::auth::Auth>>) -> rocket::response::Redirect {
    let token = auth.get_new_csrf().await;
    let uri = BASE_URI.append("/".into(), Some(format!("client_id={client_id}&response_type=code&redirect_uri={oauth_url}&scope={SCOPES}&state={token}", oauth_url=crate::rocket::auth::discord::OAUTH_URL, client_id=auth.twitch.client_id).into()));
    rocket::response::Redirect::temporary(uri)
}
