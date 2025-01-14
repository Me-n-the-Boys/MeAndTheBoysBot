mod client;
mod twitch_client;
mod rocket;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

#[tokio::main]
async fn main() -> ::anyhow::Result<()>{
    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    dotenv::dotenv()?;

    let stdout = tracing_subscriber::fmt::Layer::default();

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
        .with(stdout); // log to stdout
        // .with(webhook) //publish to discord
        // .with(ht); // publish to honeycomb backend

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

    let mut js = tokio::task::JoinSet::new();
    let (rocket, twitch) = rocket::launch().await?;
    js.spawn(async {rocket.launch().await?; Ok(())});
    js.spawn(client::init_client(twitch));
    while let Some(task) = js.join_next().await {
        let () = task??;
    }
    
    Ok(())
}