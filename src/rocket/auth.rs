pub mod discord;
pub mod twitch;

use std::sync::Arc;
use base64::Engine;

pub(super) const CSRF_TOKEN_LENGTH: usize = 32;

#[non_exhaustive]
pub struct Auth {
    pub(super) csrf_tokens: scc::HashIndex<[u8; CSRF_TOKEN_LENGTH], std::time::Instant>,
    pub twitch: crate::twitch_client::Twitch,
    pub discord: discord::Discord,
}
impl Auth {
    pub(super) async fn new(rocket: rocket::Rocket<rocket::Build>) -> ::anyhow::Result<(rocket::Rocket<rocket::Build>, Arc<Self>, serenity::Client, (tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>))> {
        let (rocket, twitch) = crate::twitch_client::create_twitch_client(rocket).await?;
        let slf = Arc::new(Self {
            csrf_tokens: scc::HashIndex::new(),
            twitch,
            discord: discord::Discord::new().await?,
        });
        let discord = crate::client::init_client(slf.clone()).await?;
        let rocket = rocket.manage(slf.clone());
        let refresh = refresh_tokens(slf.clone());
        Ok((rocket, slf, discord, refresh))
    }
    pub async fn get_new_csrf(&self) -> String {
        let time = std::time::Instant::now();
        let mut csrf:[u8; CSRF_TOKEN_LENGTH] = rand::random();
        loop {
            match self.csrf_tokens.insert_async(csrf, time).await{
                Ok(_) => break,
                Err(_) => {
                    //Collision?, generate a new token
                    csrf = rand::random();
                }
            }
        }
        base64::engine::general_purpose::URL_SAFE.encode(&csrf)
    }
}


fn refresh_tokens(auth: std::sync::Arc<Auth>) -> (tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60*60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    let jh = tokio::spawn(async move {
        let auth = auth;
        let loop_work = ||async {
            tracing::info!("Refreshing Twitch Tokens");
            let mut token = auth.twitch.auth.authentications.first_entry_async().await;
            while let Some(mut entry) = token {
                let user_id = entry.key().clone();
                let valid = !entry.check_valid(&auth.twitch).await;
                token = entry.next_async().await;
                if !valid {
                    auth.twitch.auth.authentications.remove_async(&user_id).await;
                }
            }
            tracing::info!("Done Refreshing Twitch Tokens");
        };
        loop {
            tokio::select! {
            biased;
            _ = &mut receiver => break,
            _ = interval.tick() => {
                loop_work().await;
            }
        }
        }});
    (jh, sender)
}