use std::{
    io::{BufReader, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
};

use orfail::OrFail;

use crate::json_rpc_types::{Request, Response};

#[derive(Debug, clap::Args)]
pub struct BatchCallCommand {
    #[clap(short, long)]
    server: String,

    requests: Vec<Request>,
}

impl BatchCallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let server_addr = self.server.to_socket_addrs().or_fail()?.next().or_fail()?;
        let socket = TcpStream::connect(server_addr)
            .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))?;
        socket.set_nodelay(true).or_fail()?;

        let mut writer = BufWriter::new(socket);
        serde_json::to_writer(&mut writer, &self.requests).or_fail()?;
        writer.flush().or_fail()?;

        if self.requests.iter().all(|r| r.id.is_none()) {
            return Ok(());
        }

        let mut reader = BufReader::new(writer.into_inner().or_fail()?);
        let responses: Vec<Response> = serde_json::from_reader(&mut reader).or_fail()?;

        println!("{}", serde_json::to_string_pretty(&responses).or_fail()?);
        Ok(())
    }
}
