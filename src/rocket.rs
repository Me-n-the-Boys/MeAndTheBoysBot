mod webhook;
mod oauth;

use std::net::{IpAddr, Ipv4Addr};
use crate::twitch_client::Twitch;

pub(in super) async fn launch() -> anyhow::Result<(rocket::Rocket<rocket::Build>, std::sync::Arc<Twitch>)> {
    let config = rocket::config::Config{
        address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        ..rocket::config::Config::release_default()
    };
    let rocket = rocket::custom(config)
        .mount("/", rocket::routes![webhook::webhook, oauth::new_oauth, oauth::oauth_ok, oauth::oauth_err])
    ;

    super::twitch_client::create_twitch_client(rocket).await
}
