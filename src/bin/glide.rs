// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{borrow::Borrow, path::PathBuf, sync::mpsc, time::Duration};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use glide_wm::{
    actor::server::{self, AsciiEscaped, Request, Response, ServiceRequest},
    config::{Config, config_path_default},
    sys::{
        bundle::{self, BundleError},
        message_port::{RemoteMessagePort, RemotePortCreateError, SendError},
    },
};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;

const TIMEOUT: Duration = Duration::from_millis(1000);

/// Client to control a running Glide server.
#[derive(Parser)]
#[command(version, name = "glide")]
struct Opt {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Launch Glide.
    Launch(CmdLaunch),
    #[command(subcommand)]
    Service(CmdService),
    #[command()]
    Ping(CmdPing),
    #[command()]
    Config(CmdConfig),
}

/// Manage Glide as a system service.
#[derive(Subcommand, Clone)]
enum CmdService {
    /// Add Glide to login items.
    Install,
    /// Remove Glide from login items.
    Uninstall,
}

/// Checks if the server is running.
#[derive(Parser, Clone)]
struct CmdPing {
    msg: Option<String>,
}

/// Launch Glide with optional configuration.
#[derive(Parser, Clone)]
struct CmdLaunch {
    /// Path to a custom config file.
    #[arg(long, short)]
    config: Option<PathBuf>,
}

/// Manage server config.
#[derive(Parser, Clone)]
struct CmdConfig {
    /// Path to a custom config file.
    #[arg(long, short, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    action: ConfigSubcommand,
}

#[derive(Subcommand, Clone)]
enum ConfigSubcommand {
    /// Read the config file and update the config on the running server.
    Update(CmdUpdate),
    /// Check the config file for errors.
    Verify,
}

/// Updates the server config by parsing the config file on disk.
///
/// The config file lives at ~/.glide.toml.
#[derive(Parser, Clone)]
struct CmdUpdate {
    /// Watch for config changes, continuously updating the file.
    #[arg(long)]
    watch: bool,
}

fn main() -> Result<(), anyhow::Error> {
    let opt: Opt = Parser::parse();

    if let Command::Launch(cmd) = opt.command {
        match bundle::glide_bundle() {
            Err(BundleError::NotInBundle) => bail!(
                "Not running in a bundle.
                \n\
                To run glide from the command line, use `cargo run` or start glide_server directly."
            ),
            Err(BundleError::BundleNotGlide { identifier }) => {
                bail!("Don't recognize bundle identifier {identifier}")
            }
            Ok(bundle) => {
                let config_path = cmd.config.as_ref();
                let config_result = Config::load(config_path.unwrap_or(&config_path_default()));
                if let Err(e) = config_result {
                    bail!("Config is invalid; refusing to launch Glide:\n{e}");
                }
                if Client::new().is_ok() {
                    bail!(
                        "Glide appears to be running already.
                        \n\
                        Tip: The default key binding to exit Glide is Alt+Shift+E."
                    );
                }
                let mut args: Vec<String> = Vec::new();
                if let Some(path) = &cmd.config {
                    args.push("--config".to_string());
                    args.push(path.to_string_lossy().into_owned());
                }
                bundle::launch(&bundle, &args)?;
                eprintln!(
                    "Glide is starting.
                    \n\
                    Tip: Use Alt+Z to start managing the current space.\n\
                    Tip: Use Alt+Shift+E to exit Glide."
                );
                return Ok(());
            }
        }
    }

    // Remaining commands all depend on the server running.
    let mut client = Client::new().context("Could not find server")?;

    match opt.command {
        Command::Launch(_) => unreachable!(), // handled above
        Command::Service(req) => {
            let (req, verb) = match req {
                CmdService::Install => (ServiceRequest::Install, "registered"),
                CmdService::Uninstall => (ServiceRequest::Uninstall, "unregistered"),
            };
            let response = client.send(Request::Service(req))?;
            match response {
                Response::Success => println!("Glide was {verb} as a service"),
                Response::Error(e) => bail!("{e}"),
                _ => bail!("Unexpected response"),
            }
        }
        Command::Ping(send) => {
            let response = client.send(Request::Ping(send.msg.unwrap_or_default()))?;
            match response {
                Response::Pong(data) => eprintln!("Got response {data}"),
                _ => bail!("Unexpected response"),
            }
        }
        Command::Config(CmdConfig {
            config,
            action: ConfigSubcommand::Update(CmdUpdate { watch }),
        }) => {
            let path = config.unwrap_or(config_path_default());
            let mut update_config = || {
                if !path.exists() {
                    eprintln!("Warning: Config file missing; will load defaults");
                }
                let config = match Config::load(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("{e}\n");
                        return;
                    }
                };
                let request = Request::UpdateConfig(config);
                loop {
                    match client.send(&request) {
                        Ok(Response::Success) => eprintln!("config updated"),
                        Ok(resp) => eprintln!("Unexpected response: {resp:?}"),
                        Err(ClientError::SendError(SendError::InvalidPort)) => {
                            eprintln!("Could not send to server; will attempt reconnect");
                            client = Client::new().expect("Could not find server");
                            continue;
                        }
                        Err(e) => eprintln!("Error: {e}"),
                    }
                    break;
                }
            };
            if watch {
                let (tx, rx) = mpsc::channel();
                let mut debouncer = new_debouncer(Duration::from_millis(50), tx)?;
                debouncer.watcher().watch(&config_path_default(), RecursiveMode::NonRecursive)?;
                update_config();
                for event in rx {
                    event?;
                    update_config();
                }
            } else {
                update_config();
            }
        }
        Command::Config(CmdConfig {
            config,
            action: ConfigSubcommand::Verify,
        }) => {
            let path = config.unwrap_or(config_path_default());
            if !path.exists() {
                bail!("Config file missing");
            }
            if let Err(e) = Config::load(&path) {
                eprintln!("{e}");
                std::process::exit(1);
            }
            eprintln!("config ok");
        }
    }

    Ok(())
}

struct Client {
    port: RemoteMessagePort,
}

#[derive(thiserror::Error, Debug)]
enum ClientError {
    #[error("Serialization error")]
    SerializationError(#[source] anyhow::Error),
    #[error("Sending message failed")]
    SendError(#[source] SendError),
}

impl Client {
    fn new() -> Result<Self, RemotePortCreateError> {
        Ok(Self {
            port: RemoteMessagePort::new(server::PORT_NAME)?,
        })
    }

    fn send(&self, req: impl Borrow<Request>) -> Result<Response, ClientError> {
        let msg = ron::ser::to_string(req.borrow())
            .context("Serializing message failed")
            .map_err(ClientError::SerializationError)?;
        let resp = self
            .port
            .send_message(0, msg.as_bytes(), TIMEOUT)
            .map_err(ClientError::SendError)?;
        let response = ron::de::from_bytes(&resp)
            .with_context(|| format!("Response: \"{}\"", AsciiEscaped(&resp)))
            .context("Deserializing response failed")
            .map_err(ClientError::SerializationError)?;
        Ok(response)
    }
}
