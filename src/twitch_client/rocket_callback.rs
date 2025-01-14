use crate::twitch_client::{get_base_url, Twitch, TWITCH_WS_SECRET_STRING};
pub(super) struct TwitchRocketCallback{
    pub(super) client: std::sync::Arc<Twitch>
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
        match self.client.client.update_conduit_shards(
            &self.client.conduit.id,
            vec![
                twitch_api::eventsub::Shard::new(
                    0,
                    twitch_api::eventsub::Transport::webhook(
                        format!("{}twitch/eventsub", get_base_url()),
                        TWITCH_WS_SECRET_STRING.clone(),
                    ),
                )
            ],
            &self.client.access_token
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