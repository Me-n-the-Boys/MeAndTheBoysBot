mod client;

use tokio::runtime::Runtime;
use tracing::instrument;
use tracing::level_filters::LevelFilter;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

///Return a static instance of a Runtime.
/// This will only create a Runtime once, that can then be reused.
#[must_use = "What is the point of getting a Runtime, and not doing anything?"]
#[allow(static_mut_refs)]
pub fn get_rt() -> &'static Runtime {
    static mut RT: Option<Runtime> = None;
    unsafe { &mut RT }.get_or_insert_with(|| tokio::runtime::Runtime::new().unwrap())
}

fn main() {
    //Tie the Runtime to this main.
    //Must use for tokio::spawn to not panic
    let _rtg = get_rt().enter();

    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    dotenv::dotenv().expect("Failed to load .env file");

    let stdout = tracing_subscriber::fmt::Layer::default();

    let subscriber = registry::Registry::default() // provide underlying span data store
        .with(LevelFilter::INFO) // filter out low-level debug tracing (eg tokio executor)
        .with(stdout); // log to stdout
        // .with(webhook) //publish to discord
        // .with(ht); // publish to honeycomb backend

    tracing::subscriber::set_global_default(subscriber).expect("setting global default failed");

    start_client()
}

pub fn is_logging_enabled(key: String) -> bool {
    dotenv::var(key)
        .map(|v| v.parse::<bool>().unwrap_or(false))
        .unwrap_or(false)
}

#[instrument]
fn start_client() {
    // eaze_tracing_honeycomb::register_dist_tracing_root(eaze_tracing_honeycomb::TraceId::new(), None).unwrap();
    tracing::info!("Client Startup");
    get_rt().block_on(client::init_client());
    tracing::info!("Bye!");
}
