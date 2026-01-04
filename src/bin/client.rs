// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use glide_wm::{
    actor::server::{self, AsciiEscaped, Request, Response},
    sys::message_port::{RemoteMessagePort, RemotePortCreateError},
};

const TIMEOUT: Duration = Duration::from_millis(1000);

#[derive(Parser)]
struct Opt {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    #[command()]
    Send(SendMsg),
}

#[derive(Parser, Clone)]
struct SendMsg {
    msg: String,
}

fn main() -> Result<(), anyhow::Error> {
    let opt: Opt = Parser::parse();

    let client = Client::new().expect("Could not find remote");

    match opt.command {
        Command::Send(send) => {
            let response = client.send(Request::Ping(send.msg))?;
            match response {
                Response::Pong(data) => eprintln!("Got response {data}"),
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
