use std::net::{TcpStream, ToSocketAddrs};

use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams, RpcClient};
use orfail::{Failure, OrFail};

#[derive(Debug, clap::Args)]
pub struct CallCommand {
    server_addr: String,

    #[clap(short, long)]
    method: String,

    #[clap(short, long)]
    params: Option<RequestParams>,

    #[clap(short, long)]
    id: Option<RequestId>,
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

            let request = RequestObject {
                jsonrpc: JsonRpcVersion::V2,
                method: self.method,
                params: self.params,
                id: self.id,
            };

            if let Some(response) = client.call(&request).or_fail()? {
                println!("{}", serde_json::to_string(&response).or_fail()?);
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
