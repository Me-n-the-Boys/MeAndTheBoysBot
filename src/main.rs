mod client;
mod twitch_client;
mod rocket;
mod discord_client;

use ::rocket::{Orbit, Rocket};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

static SHUTDOWN: tokio::sync::Notify = tokio::sync::Notify::const_new();

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

    let shutdown_watcher = tokio::spawn(async{
        let mut js = tokio::task::JoinSet::new();
        js.spawn(async {
            ::tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
            None
        });
        #[cfg(unix)]
        {
            let mut handler = ::tokio::signal::unix::signal(::tokio::signal::unix::SignalKind::terminate()).expect("Could not register SIGTERM handler");
            js.spawn(async move{ handler.recv().await});
        }
        #[cfg(windows)]
        {
            let mut handler = ::tokio::signal::windows::ctrl_close().expect("Could not register CTRL-SHUTDOWN handler");
            js.spawn(async move{ handler.recv().await});
        }
        let _ = js.join_next().await; //We don't care, if a thread panicked. If something happened here, we assume that the program should shut down.
        js.abort_all();
        SHUTDOWN.notify_waiters();
        while let Some(v) = js.join_next().await {
            match v {
                Ok(_) => {},
                Err(err) => {
                    match err.try_into_panic() {
                        Ok(v) => {
                            std::panic::resume_unwind(v)
                        }
                        Err(_) => {
                            //we assume, that the error here was due to the task being cancelled.
                        }
                    }
                }
            }
        }
    });

    let mut js = tokio::task::JoinSet::<::anyhow::Result<()>>::new();

    let (rocket, mut discord, refresh) = rocket::launch().await?;
    let rocket = rocket.attach(Shutdown);
    {
        tokio::task::spawn(async {
            SHUTDOWN.notified().await;
            let (refresh_handle, refresh_exit) = refresh;
            if let Err(()) = refresh_exit.send(()) {
                tracing::error!("Failed to send abort signal to refresh task");
            }
            if let Err(refresh_err) = refresh_handle.await {
                tracing::error!("refresh task errored: {refresh_err}");
            }
        });
    }
    js.spawn(async {rocket.launch().await?; Ok(())});
    js.spawn(async move { discord.start_autosharded().await?; Ok(())});
    while let Some(task) = js.join_next().await {
        match task.map_or_else(|err|Err(::anyhow::format_err!("{err}")), |res| res) {
            Ok(()) => {},
            Err(mut err) => {
                shutdown_watcher.abort();
                tracing::error!("Error: {err}");
                SHUTDOWN.notify_waiters();
                let _ = shutdown_watcher.await;
                return Err(err);
            }
        }
    }
    
    Ok(())
}

struct Shutdown;
#[::rocket::async_trait]
impl ::rocket::fairing::Fairing for Shutdown {
    fn info(&self) -> ::rocket::fairing::Info {
        ::rocket::fairing::Info {
            name: "Shutdown",
            kind: ::rocket::fairing::Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let shutdown = rocket.shutdown();
        tokio::spawn(async move{
            SHUTDOWN.notified().await;
            shutdown.notify();
        });
    }
}