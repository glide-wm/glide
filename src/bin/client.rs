use std::{sync::mpsc, time::Duration};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use glide_wm::{
    actor::server::{self, AsciiEscaped, Request, Response},
    config::{self, Config},
    sys::message_port::{RemoteMessagePort, RemotePortCreateError},
};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;

const TIMEOUT: Duration = Duration::from_millis(1000);

/// Client to control a running Glide server.
#[derive(Parser)]
struct Opt {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
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

    let client = Client::new().context("Could not find server")?;

    match opt.command {
        Command::Ping(send) => {
            let response = client.send(Request::Ping(send.msg.unwrap_or_default()))?;
            match response {
                Response::Pong(data) => eprintln!("Got response {data}"),
                _ => bail!("Unexpected response"),
            }
        }
        Command::Config(CmdConfig::Update(CmdUpdate { watch })) => {
            let update_config = || {
                let config = Config::read(&config::config_file())?;
                match client.send(Request::UpdateConfig(config))? {
                    Response::Success => eprintln!("config updated"),
                    _ => bail!("Unexpected response"),
                }
                Ok(())
            };
            if watch {
                let (tx, rx) = mpsc::channel();
                let mut debouncer = new_debouncer(Duration::from_millis(100), tx)?;
                debouncer.watcher().watch(&config::config_file(), RecursiveMode::NonRecursive)?;
                if let Err(e) = update_config() {
                    eprintln!("Error: {e}");
                }
                for event in rx {
                    event?;
                    if let Err(e) = update_config() {
                        eprintln!("Error: {e}");
                    }
                }
            } else {
                update_config()?;
            }
        }
    }

    Ok(())
}

struct Client {
    port: RemoteMessagePort,
}

impl Client {
    fn new() -> Result<Self, RemotePortCreateError> {
        Ok(Self {
            port: RemoteMessagePort::new(server::PORT_NAME)?,
        })
    }

    fn send(&self, req: Request) -> Result<Response, anyhow::Error> {
        let msg = ron::ser::to_string(&req).context("Serializing message failed")?;
        let resp = self
            .port
            .send_message(0, msg.as_bytes(), TIMEOUT)
            .context("Sending message failed")?;
        let response = ron::de::from_bytes(&resp)
            .with_context(|| format!("Response: \"{}\"", AsciiEscaped(&resp)))
            .context("Deserializing response failed")?;
        Ok(response)
    }
}
