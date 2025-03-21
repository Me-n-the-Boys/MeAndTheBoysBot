mod client;
mod twitch_client;
mod rocket;
mod discord_client;

use ::rocket::{Orbit, Rocket};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

static SHUTDOWN: tokio::sync::Notify = tokio::sync::Notify::const_new();
static RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(||{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Could not create tokio runtime")
});

const FAIL2BAN_TARGET:&str = "fail2ban";
struct Fail2BanFilter;
impl<S>tracing_subscriber::layer::Filter<S> for Fail2BanFilter {
    fn enabled(&self, meta: &tracing::Metadata<'_>, _: &tracing_subscriber::layer::Context<'_, S>) -> bool {
        meta.target() == FAIL2BAN_TARGET
    }
    fn callsite_enabled(&self, meta: &'static tracing::Metadata<'static>) -> tracing::subscriber::Interest {
        if meta.target() == FAIL2BAN_TARGET {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
    }
}

fn main() -> ::anyhow::Result<()>{
    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    dotenvy::dotenv()?;

    {
        let path = std::env::var_os("LOG_PATH").unwrap_or_else(|| "logs".to_string().into());
        let mut path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).expect("Failed to create log directory");
        path.push("rocket.log");
        let rocket_logfile = std::fs::File::create(&path).expect("Failed to create rocket log file");
        path.pop();
        path.push("fail2ban.log");
        let fail2ban_logfile = std::fs::File::create(&path).expect("Failed to create fail2ban log file");

        use tracing_subscriber::Layer;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let registry = tracing_subscriber::registry();
        #[cfg(tokio_unstable)]
        let registry = registry.with(console_subscriber::spawn());
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_target(true)
                    .with_ansi(false)
                    .with_writer(rocket_logfile)
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_target(true)
                    .with_ansi(false)
                    .with_writer(fail2ban_logfile)
                    .with_filter(Fail2BanFilter{})
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
            )
            .init();
        log::info!("Initialized logging");
    }

    async_main()
}

#[tokio::main]
async fn async_main() -> ::anyhow::Result<()>{
    let _a = get_db().await;

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

pub(crate) async fn get_db<'a>() -> sqlx::PgPool {
    static MYSQL: tokio::sync::OnceCell<sqlx::PgPool> = tokio::sync::OnceCell::const_new();
    MYSQL.get_or_init(||async {
        let options = sqlx::postgres::PgConnectOptions::new();
        let pool = sqlx::Pool::connect_with(options).await.expect("Failed to connect to postgres");
        log::info!("Connected to postgres");
        pool
    }).await.clone()
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

//TODO: Use .cast_signed and .cast_unsigned when they are stable?
// https://github.com/rust-lang/rust/issues/125882
fn converti(num: u64) -> i64 {
    // num.cast_signed()
    i64::from_ne_bytes(num.to_ne_bytes())
}
//TODO: Use .cast_signed and .cast_unsigned when they are stable?
// https://github.com/rust-lang/rust/issues/125882
fn convertu(num: i64) -> u64 {
    // num.cast_unsigned()
    u64::from_ne_bytes(num.to_ne_bytes())
}
