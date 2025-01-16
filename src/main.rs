mod client;
mod twitch_client;
mod rocket;
mod discord_client;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

#[tokio::main]
async fn main() -> ::anyhow::Result<()>{
    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    dotenv::dotenv()?;

    let stdout = tracing_subscriber::fmt::Layer::default();

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(tracing_subscriber::EnvFilter::from_default_env()) // filter spans based on env var
        .with(stdout); // log to stdout
        // .with(webhook) //publish to discord
        // .with(ht); // publish to honeycomb backend

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

    let mut js = tokio::task::JoinSet::<::anyhow::Result<()>>::new();

    //Get initial rocket instance
    let rocket = rocket::launch()?;
    //Start Twitch client and add necessary rocket configuration
    let (rocket, twitch, (refresh_handle, refresh_abort)) = twitch_client::create_twitch_client(rocket).await?;
    //Configure Discord client
    let mut client = client::init_client(twitch).await?;
    //And add necessary rocket configuration
    let dc_client = discord_client::DiscordClient::new(&client);
    let rocket = rocket.manage(dc_client);
    //Start the rocket and client
    js.spawn(async {rocket.launch().await?; Ok(())});
    js.spawn(async move { client.start_autosharded().await?; Ok(())});
    while let Some(task) = js.join_next().await {
        match task.map_or_else(|err|Err(::anyhow::format_err!("{err}")), |res| res) {
            Ok(()) => {},
            Err(mut err) => {
                if let Err(()) = refresh_abort.send(()) {
                    err = err.context("Failed to send abort signal to refresh task");
                }
                if let Err(refresh_err) = refresh_handle.await {
                    err = err.context(anyhow::format_err!("refresh task errored: {refresh_err}"));
                }
                tracing::error!("Error: {err}");
                return Err(err);
            }
        }
    }
    
    Ok(())
}