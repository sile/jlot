use std::net::{TcpStream, ToSocketAddrs};

use jsonlrpc::{RequestObject, RpcClient};
use orfail::OrFail;

#[derive(Debug, clap::Args)]
pub struct BatchCallCommand {
    #[clap(short, long)]
    server: String,

    requests: Vec<RequestObject>,
}

impl BatchCallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let server_addr = self.server.to_socket_addrs().or_fail()?.next().or_fail()?;
        let socket = TcpStream::connect(server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))?;
        socket.set_nodelay(true).or_fail()?;
        let mut client = RpcClient::new(socket);

        let responses = client.batch_call(&self.requests).or_fail()?;
        println!("{}", serde_json::to_string(&responses).or_fail()?);

        Ok(())
    }
}
