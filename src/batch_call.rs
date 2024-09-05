use std::{
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
};

use jsonlrpc::{RequestObject, RpcClient};
use orfail::{Failure, OrFail};

#[derive(Debug, clap::Args)]
pub struct BatchCallCommand {
    server_addr: String,

    requests: BatchRequest,
}

impl BatchCallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let mut last_connect_error = None;
        for server_addr in self.server_addr.to_socket_addrs().or_fail()? {
            let socket = match TcpStream::connect(server_addr)
                .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))
            {
                Ok(socket) => socket,
                Err(error) => {
                    last_connect_error = Some(error);
                    continue;
                }
            };
            socket.set_nodelay(true).or_fail()?;
            let mut client = RpcClient::new(socket);

            let responses = client.batch_call(&self.requests.0).or_fail()?;
            println!("{}", serde_json::to_string(&responses).or_fail()?);

            return Ok(());
        }

        Err(last_connect_error.unwrap_or_else(|| {
            Failure::new(format!(
                "Failed to resolve server address: {:?}",
                self.server_addr,
            ))
        }))
    }
}

#[derive(Debug, Clone)]
struct BatchRequest(Vec<RequestObject>);

impl FromStr for BatchRequest {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let requests: Vec<RequestObject> = serde_json::from_str(s)?;
        Ok(Self(requests))
    }
}