mod api;
mod cli;
mod commands;
mod credentials;
mod models;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Auth {
            client_id,
            client_secret,
            url,
        } => commands::auth::run(client_id, client_secret, url),
        Commands::Update { path, name } => {
            commands::update::run(path, name.as_deref()).await
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}
