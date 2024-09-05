use std::net::{TcpStream, ToSocketAddrs};

use jsonlrpc::{MaybeBatch, RequestObject, RpcClient};
use orfail::{Failure, OrFail};

#[derive(Debug, clap::Args)]
pub struct CallCommand {
    server_addr: String,
    request: MaybeBatch<RequestObject>,
}

impl CallCommand {
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

            match self.request {
                MaybeBatch::Single(request) => {
                    if let Some(response) = client.call(&request).or_fail()? {
                        println!("{}", serde_json::to_string(&response).or_fail()?);
                    }
                }
                MaybeBatch::Batch(requests) => {
                    let responses = client.batch_call(&requests).or_fail()?;
                    if !responses.is_empty() {
                        println!("{}", serde_json::to_string(&responses).or_fail()?);
                    }
                }
            }

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
