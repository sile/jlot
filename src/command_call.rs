use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
};

use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams, ResponseObject};
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
        // TODO: use RpcClient
        let is_notification = self.id.is_none();

        let server_addr = self.server.to_socket_addrs().or_fail()?.next().or_fail()?;
        let socket = TcpStream::connect(server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))?;
        socket.set_nodelay(true).or_fail()?;

        let mut writer = BufWriter::new(socket);
        serde_json::to_writer(
            &mut writer,
            &RequestObject {
                jsonrpc: JsonRpcVersion::V2,
                method: self.method,
                params: self.params,
                id: self.id,
            },
        )
        .or_fail()?;
        writer.write_all(b"\n").or_fail()?;
        writer.flush().or_fail()?;

        if is_notification {
            return Ok(());
        }

        let mut reader = BufReader::new(writer.into_inner().or_fail()?);
        let mut line = String::new();
        reader.read_line(&mut line).or_fail()?;
        let response: ResponseObject = serde_json::from_str(&line).or_fail()?;

        println!("{}", serde_json::to_string_pretty(&response).or_fail()?);
        Ok(())
    }
}
