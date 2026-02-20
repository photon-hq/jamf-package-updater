use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jamf-package-updater")]
#[command(about = "Simplify package updates in Jamf Pro")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Store Jamf Pro API credentials
    Auth {
        /// Jamf Pro API client ID
        #[arg(long)]
        client_id: String,

        /// Jamf Pro API client secret
        #[arg(long)]
        client_secret: String,

        /// Jamf Pro instance URL (e.g. https://example.jamfcloud.com)
        #[arg(long)]
        url: String,
    },

    /// Update a package in Jamf Pro and reassign it to all policies that used it
    Update {
        /// Path to a .pkg or .dmg file
        path: PathBuf,

        /// Package name to match in Jamf Pro (defaults to file stem)
        #[arg(long)]
        name: Option<String>,

        /// Package priority in Jamf Pro (0–20). Overrides the existing value
        /// for updates and the default (3) for new packages.
        #[arg(long)]
        priority: Option<i32>,
    },
}
