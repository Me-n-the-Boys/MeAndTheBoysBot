pub(super) struct TwitchRocketCallback{
    pub client: twitch_api::HelixClient<'static, reqwest::Client>,
    pub access_token: twitch_api::twitch_oauth2::AppAccessToken,
    pub conduit: twitch_api::eventsub::Conduit,
}

#[rocket::async_trait]
impl rocket::fairing::Fairing for TwitchRocketCallback {
    fn info(&self) -> rocket::fairing::Info {
        rocket::fairing::Info{
            name: "Twitch Rocket Callback for initializing Twitch Client",
            kind: rocket::fairing::Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &rocket::Rocket<rocket::Orbit>) {
        match self.client.update_conduit_shards(
            &self.conduit.id,
            vec![
                twitch_api::eventsub::Shard::new(
                    0,
                    twitch_api::eventsub::Transport::webhook(
                        const_format::concatcp!(crate::rocket::BASE_SCHEME, "://", crate::rocket::BASE_URL, "/twitch/eventsub"),
                        super::TWITCH_WS_SECRET_STRING.clone(),
                    ),
                )
            ],
            &self.access_token
        ).await {
            Err(e) => {
                tracing::error!("Failed to update conduit shards: {e:?}");
                rocket.shutdown().await;
            }
            Ok(v) if !v.errors.is_empty() => {
                tracing::error!("Failed to update some conduit shards: {v:?}");
                rocket.shutdown().await;
            },
            Ok(v) => {
                let shards = v.shards;
                tracing::info!("Updated conduit shards: {shards:?}");
            }
        }
    }
}