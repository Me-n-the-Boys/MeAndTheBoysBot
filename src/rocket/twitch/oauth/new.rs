use std::sync::Arc;

#[rocket::get("/twitch/new_oauth")]
pub async fn new_oauth<'r>(twitch: &rocket::State<Arc<crate::twitch_client::Twitch>>) -> rocket::response::Redirect {
    rocket::response::Redirect::temporary(twitch.get_new_oath().await)
}
