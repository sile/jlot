use std::net::{TcpStream, ToSocketAddrs};

use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams, RpcClient};
use orfail::OrFail;

#[derive(Debug, clap::Args)]
pub struct CallCommand {
    #[clap(short, long)]
    server: String,

    #[clap(short, long)]
    method: String,

    #[clap(short, long)]
    params: Option<RequestParams>,

    #[clap(short, long)]
    id: Option<RequestId>,
}

impl CallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let server_addr = self.server.to_socket_addrs().or_fail()?.next().or_fail()?;
        let socket = TcpStream::connect(server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))?;
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

        Ok(())
    }
}
