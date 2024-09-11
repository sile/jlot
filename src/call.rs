use std::net::{TcpStream, ToSocketAddrs};

use jsonlrpc::{MaybeBatch, RequestObject, ResponseObject, RpcClient};
use orfail::{Failure, OrFail};

/// Execute a JSON-RPC call.
#[derive(Debug, clap::Args)]
pub struct CallCommand {
    /// JSON-RPC server address or hostname.
    server_addr: String,

    /// JSON-RPC request object or array of request objects (for batch call).
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

            let is_notification = self.request.iter().all(|r| r.id.is_none());
            match is_notification {
                true => {
                    client.cast(&self.request).or_fail()?;
                }
                false => {
                    let response: MaybeBatch<ResponseObject> =
                        client.call(&self.request).or_fail()?;
                    println!("{}", serde_json::to_string(&response).or_fail()?);
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
