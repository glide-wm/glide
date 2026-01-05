// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use glide_wm::{
    actor::server::{self, AsciiEscaped, Request, Response},
    config::{self, Config},
    sys::message_port::{RemoteMessagePort, RemotePortCreateError},
};

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
    Ping(Ping),
    #[command(subcommand)]
    Config(CmdConfig),
}

/// Checks if the server is running.
#[derive(Parser, Clone)]
struct Ping {
    msg: Option<String>,
}

/// Updates the server config by parsing the config file on disk.
///
/// The config file lives at ~/.glide.toml.
#[derive(Subcommand, Clone)]
enum CmdConfig {
    Update,
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
        Command::Config(CmdConfig::Update) => {
            let config = Config::read(&config::config_file())?;
            match client.send(Request::UpdateConfig(config))? {
                Response::Success => eprintln!("config updated"),
                _ => bail!("Unexpected response"),
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
