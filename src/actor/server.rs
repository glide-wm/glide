// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message server that handles requests from the Glide CLI.

use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    future::pending,
    rc::Rc,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tracing::{Span, error, info, instrument, warn};

use crate::{
    actor::wm_controller,
    config::Config,
    sys::message_port::{LocalMessagePort, LocalPortCreateError},
};

pub const PORT_NAME: &str = "org.glidewm.server";

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Ping(String),
    UpdateConfig(Config),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Pong(String),
    Success,
}

pub struct MessageServer {
    #[expect(unused)]
    port: LocalMessagePort,
    #[expect(unused)]
    state: Rc<RefCell<State>>,
}

struct State {
    wm_tx: wm_controller::Sender,
}

impl MessageServer {
    pub fn new(name: &str, wm_tx: wm_controller::Sender) -> Result<Self, LocalPortCreateError> {
        let state = Rc::new(RefCell::new(State { wm_tx }));
        let state_ = state.clone();
        Ok(MessageServer {
            port: LocalMessagePort::new(name, move |id, msg| {
                state_.borrow_mut().handle_message(id, msg)
            })?,
            state,
        })
    }

    pub async fn run(self) {
        // For now just don't return.
        pending().await
    }
}

impl State {
    fn handle_message(&mut self, id: i32, message: &[u8]) -> Vec<u8> {
        let Ok(request) = ron::de::from_bytes::<Request>(message) else {
            warn!(
                "Got invalid message with id {id} on port: \"{}\"",
                AsciiEscaped(message)
            );
            return vec![];
        };
        info!("Got message {id} on port: {request:?}");
        let response = self.on_request(request);
        match ron::ser::to_string(&response) {
            Ok(bytes) => bytes.into_bytes(),
            Err(e) => {
                error!("Failed to serialize response: {e}");
                vec![]
            }
        }
    }

    #[instrument(skip(self))]
    fn on_request(&mut self, request: Request) -> Response {
        match request {
            Request::Ping(payload) => {
                let resp = payload.chars().into_iter().rev().collect();
                Response::Pong(resp)
            }
            Request::UpdateConfig(config) => {
                _ = self.wm_tx.send((
                    Span::current(),
                    wm_controller::WmEvent::ConfigUpdated(Arc::new(config)),
                ));
                Response::Success
            }
        }
    }
}

pub struct AsciiEscaped<'a>(pub &'a [u8]);

impl Display for AsciiEscaped<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{}", std::ascii::escape_default(*byte))?;
        }
        Ok(())
    }
}
