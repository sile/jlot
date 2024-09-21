use std::net::TcpStream;

use jsonlrpc::{MaybeBatch, RequestObject, ResponseObject, RpcClient};
use orfail::OrFail;

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
        let socket = TcpStream::connect(&self.server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{}': {e}", self.server_addr))?;
        socket.set_nodelay(true).or_fail()?;
        let mut client = RpcClient::new(socket);

        let is_notification = self.request.iter().all(|r| r.id.is_none());
        match is_notification {
            true => {
                client.cast(&self.request).or_fail()?;
            }
            false => {
                let response: MaybeBatch<ResponseObject> = client.call(&self.request).or_fail()?;
                println!("{}", serde_json::to_string(&response).or_fail()?);
            }
        }

        Ok(())
    }
}
