
mod twitch;
pub(self) mod csrf;
pub(self) mod base64;
mod index;
mod discord;
pub mod auth;

pub(crate) const BASE_SCHEME: &'static str = "https";
#[cfg(debug_assertions)]
pub(crate) const BASE_URL: &'static str = "debug.twitch.meandtheboys.c0d3m4513r.com";

#[cfg(not(debug_assertions))]
pub(crate) const BASE_URL: &'static str = "twitch.meandtheboys.c0d3m4513r.com";

pub(in super) async fn launch() -> anyhow::Result<(rocket::Rocket<rocket::Build>, serenity::Client, (tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>))> {
    use ::base64::Engine;
    let secret_key = match dotenv::var("ROCKET_SECRET_KEY") {
        Ok(v) => match ::base64::engine::general_purpose::STANDARD.decode(v){
            Ok(v) => rocket::config::SecretKey::from(v.as_slice()),
            Err(_) => {
                ::anyhow::bail!("ROCKET_SECRET_KEY must be stored base64 encoded");
            }
        },
        Err(_) => {
            ::anyhow::bail!("ROCKET_SECRET_KEY must be set in .env");
        }
    };
    let config = rocket::config::Config{
        address: std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        ident: rocket::config::Ident::none(),
        secret_key,
        ..rocket::config::Config::release_default()
    };
    let rocket = rocket::custom(config)
        .mount("/", rocket::routes![
            index::index,
            twitch::webhook::webhook,
            twitch::oauth::new::new_oauth,
            twitch::oauth::ok::oauth_ok,
            twitch::oauth::err::oauth_err,
            discord::oauth::new::new_oauth,
            discord::oauth::ok::oauth_ok,
            discord::oauth::err::oauth_err,
        ])
    ;

    let (rocket, auth, discord, refresh) = auth::Auth::new(rocket).await?;

    Ok((rocket, discord, refresh))
}
