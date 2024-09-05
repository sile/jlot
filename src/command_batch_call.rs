use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
};

use jsonlrpc::{RequestObject, ResponseObject};
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

        let mut writer = BufWriter::new(socket);
        serde_json::to_writer(&mut writer, &self.requests).or_fail()?;
        writer.write_all(b"\n").or_fail()?;
        writer.flush().or_fail()?;

        if self.requests.iter().all(|r| r.id.is_none()) {
            return Ok(());
        }

        let mut reader = BufReader::new(writer.into_inner().or_fail()?);
        let mut line = String::new();
        reader.read_line(&mut line).or_fail()?;
        let responses: Vec<ResponseObject> = serde_json::from_str(&line).or_fail()?;

        println!("{}", serde_json::to_string_pretty(&responses).or_fail()?);
        Ok(())
    }
}
