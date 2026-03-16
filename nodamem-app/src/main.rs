use memory_store::{StoreConfig, StoreRuntime};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    init_tracing();

    println!("Nodamem starting...");

    let config = StoreConfig::from_env();
    let startup_mode = startup_mode(&config);

    match StoreRuntime::open(config.clone()).await {
        Ok(_runtime) => {
            println!(
                "Store initialized: mode={startup_mode}, path={}",
                config.local_database_path.display()
            );
        }
        Err(error) => {
            eprintln!(
                "Failed to initialize store: mode={startup_mode}, path={}, error={error}",
                config.local_database_path.display()
            );
            std::process::exit(1);
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("nodamem_app=info,memory_store=info")),
        )
        .with_target(true)
        .compact()
        .try_init();
}

fn startup_mode(config: &StoreConfig) -> &'static str {
    if config.turso_sync_config().is_some() {
        "turso-sync-configured"
    } else if config.sync_requested_without_credentials() {
        "local-only-sync-incomplete"
    } else {
        "local-only"
    }
}
