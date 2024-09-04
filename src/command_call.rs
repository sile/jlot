use std::{
    io::{BufReader, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
};

use orfail::OrFail;

use crate::json_rpc_types::{Id, Request, RequestParams, Response};

#[derive(Debug, clap::Args)]
pub struct CallCommand {
    #[clap(short, long)]
    server: String,

    #[clap(short, long)]
    method: String,

    #[clap(short, long)]
    params: Option<RequestParams>,

    #[clap(short, long)]
    id: Id,
}

impl CallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let server_addr = self.server.to_socket_addrs().or_fail()?.next().or_fail()?;
        let socket = TcpStream::connect(server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))?;
        socket.set_nodelay(true).or_fail()?;

        let mut writer = BufWriter::new(socket);
        serde_json::to_writer(
            &mut writer,
            &Request::new(self.method, self.params, self.id),
        )
        .or_fail()?;
        writer.flush().or_fail()?;

        let mut reader = BufReader::new(writer.into_inner().or_fail()?);
        let response: Response = serde_json::from_reader(&mut reader).or_fail()?;

        println!("{}", serde_json::to_string_pretty(&response).or_fail()?);
        Ok(())
    }
}
