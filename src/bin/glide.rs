// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{borrow::Borrow, sync::mpsc, time::Duration};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use glide_wm::{
    actor::server::{self, AsciiEscaped, Request, Response},
    config::{self, Config},
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
    Launch,
    #[command()]
    Ping(CmdPing),
    #[command(subcommand)]
    Config(CmdConfig),
}

/// Checks if the server is running.
#[derive(Parser, Clone)]
struct CmdPing {
    msg: Option<String>,
}

/// Commands to manage server config.
#[derive(Subcommand, Clone)]
enum CmdConfig {
    Update(CmdUpdate),
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

    if let Command::Launch = opt.command {
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
                if let Err(e) = Config::load() {
                    bail!("Config is invalid; refusing to launch Glide:\n{e}");
                }
                if Client::new().is_ok() {
                    bail!(
                        "Glide appears to be running already.
                        \n\
                        Tip: The default key binding to exit Glide is Alt+Shift+E."
                    );
                }
                bundle::launch(&bundle)?;
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
        Command::Launch => unreachable!(), // handled above
        Command::Ping(send) => {
            let response = client.send(Request::Ping(send.msg.unwrap_or_default()))?;
            match response {
                Response::Pong(data) => eprintln!("Got response {data}"),
                _ => bail!("Unexpected response"),
            }
        }
        Command::Config(CmdConfig::Update(CmdUpdate { watch })) => {
            let mut update_config = || {
                if !config::config_file().exists() {
                    eprintln!("Warning: Config file missing; will load defaults");
                }
                let config = match Config::load() {
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
                debouncer.watcher().watch(&config::config_file(), RecursiveMode::NonRecursive)?;
                update_config();
                for event in rx {
                    event?;
                    update_config();
                }
            } else {
                update_config();
            }
        }
        Command::Config(CmdConfig::Verify) => {
            if !config::config_file().exists() {
                bail!("Config file missing");
            }
            if let Err(e) = Config::load() {
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
