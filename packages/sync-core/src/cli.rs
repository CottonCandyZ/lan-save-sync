use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rand::RngCore;

use crate::{
    archive,
    config::{self, find_folder},
    engine::Engine,
    manifest,
    model::{Config, DeviceConfig, SyncAction},
};

#[derive(Debug, Parser)]
#[command(
    name = "lan-save-sync",
    version,
    about = "Safe, manual LAN folder synchronization"
)]
struct Cli {
    /// JSON configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a minimal configuration containing a new random API token.
    Init {
        #[arg(long)]
        device_id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "0.0.0.0:48123")]
        listen: String,
        #[arg(long)]
        force: bool,
    },
    /// Run the authenticated peer agent.
    Serve,
    /// Hash and print the current content of one configured folder.
    Inspect {
        #[arg(long)]
        folder: String,
    },
    /// Compare local, remote, and last-synced versions without changing files.
    Plan {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        folder: String,
    },
    /// Synchronize one folder. Conflicts require an explicit direction and confirmation.
    Sync {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        folder: String,
        #[arg(long, value_enum, default_value = "auto")]
        action: SyncAction,
        #[arg(long)]
        accept_conflict: bool,
    },
    /// List local versions created before incoming overwrites.
    History {
        #[arg(long)]
        folder: String,
    },
    /// Restore a local history version after creating another safety backup.
    Restore {
        #[arg(long)]
        folder: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        accept_overwrite: bool,
    },
}

pub async fn execute(program: &str) -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| config::default_config_path(program));

    if let Command::Init {
        device_id,
        name,
        output,
        listen,
        force,
    } = cli.command
    {
        return init(
            program,
            &device_id,
            &name,
            output.as_deref().unwrap_or(&config_path),
            &listen,
            force,
        );
    }

    let config = config::load(&config_path)?;
    let engine = Engine::new(config)?;
    match cli.command {
        Command::Serve => crate::server::serve(engine).await,
        Command::Inspect { folder } => {
            let folder = find_folder(&engine.config, &folder)?;
            print_json(&manifest::scan(folder)?)
        }
        Command::Plan { peer, folder } => print_json(&engine.plan(&peer, &folder).await?),
        Command::Sync {
            peer,
            folder,
            action,
            accept_conflict,
        } => print_json(&engine.sync(&peer, &folder, action, accept_conflict).await?),
        Command::History { folder } => {
            find_folder(&engine.config, &folder)?;
            print_json(&archive::list_history(&engine.config.data_dir, &folder)?)
        }
        Command::Restore {
            folder,
            version,
            accept_overwrite,
        } => {
            if !accept_overwrite {
                bail!(
                    "restore refused; add --accept-overwrite after reviewing the selected version"
                );
            }
            let _operation_lock =
                crate::operation_lock::OperationLock::acquire(&engine.config.data_dir)?;
            let folder_config = find_folder(&engine.config, &folder)?;
            let path = archive::history_archive_path(&engine.config.data_dir, &folder, &version)?;
            let archived = archive::inspect_archive(&path, folder_config, &engine.config.data_dir)?;
            let current = manifest::scan(folder_config)?;
            let result = archive::apply_archive(
                folder_config,
                &path,
                &archived.root_hash,
                Some(&current.root_hash),
                &engine.config.data_dir,
                engine.config.history_limit,
            )?;
            print_json(&result)
        }
        Command::Init { .. } => unreachable!(),
    }
}

fn init(
    program: &str,
    device_id: &str,
    name: &str,
    output: &Path,
    listen: &str,
    force: bool,
) -> Result<()> {
    config::validate_id(device_id, "device_id")?;
    if output.exists() && !force {
        bail!(
            "configuration already exists at {}; use --force to replace it",
            output.display()
        );
    }
    let parent = output
        .parent()
        .context("configuration output has no parent")?;
    fs::create_dir_all(parent)?;
    let data_dir = parent.join("data");
    let value = Config {
        device: DeviceConfig {
            id: device_id.to_owned(),
            name: name.to_owned(),
        },
        listen: listen.to_owned(),
        api_token: random_token(),
        data_dir,
        peers: Vec::new(),
        folders: Vec::new(),
        history_limit: 20,
    };
    fs::write(output, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    println!(
        "Created {program} configuration at {}. Add peers and folders before starting the agent.",
        output.display()
    );
    Ok(())
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
