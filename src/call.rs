use std::io::{BufRead, Write};
use std::net::{TcpStream, UdpSocket};

use orfail::OrFail;

use crate::types::{Request, Response, ServerAddr};

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("call")
        .doc("Read JSON-RPC requests from standard input and execute the RPC calls")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let server_addr: ServerAddr = noargs::arg("<SERVER>")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080")
        .take(args)
        .then(|a| a.value().parse())?;

    if args.metadata().help_mode {
        return Ok(true);
    }

    let use_udp: bool = noargs::flag("udp")
        .short('u')
        .doc("Use UDP instead of TCP (one packet per request/response)")
        .take(args)
        .is_present();

    let call_command = CallCommand {
        server_addr,
        use_udp,
    };
    call_command.run().or_fail()?;

    Ok(true)
}

struct CallCommand {
    server_addr: ServerAddr,
    use_udp: bool,
}

impl CallCommand {
    fn run(self) -> orfail::Result<()> {
        if self.use_udp {
            self.run_udp()
        } else {
            self.run_tcp()
        }
    }

    fn run_tcp(self) -> orfail::Result<()> {
        let stream = self.connect_to_server().or_fail()?;

        let stdin = std::io::stdin();
        let input_reader = std::io::BufReader::new(stdin.lock());
        let stdout = std::io::stdout();
        let mut output_writer = std::io::BufWriter::new(stdout.lock());

        let mut rpc_writer = std::io::BufWriter::new(stream.try_clone().or_fail()?);
        let mut rpc_reader = std::io::BufReader::new(stream);

        for line in input_reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;

            writeln!(rpc_writer, "{}", request.json).or_fail()?;
            rpc_writer.flush().or_fail()?;

            if request.id.is_some() {
                let mut response_line = String::new();
                let bytes_read = rpc_reader.read_line(&mut response_line).or_fail()?;
                (bytes_read > 0).or_fail_with(|()| {
                    "Failed to receive RPC response: unexpected EOF".to_owned()
                })?;

                let response = Response::parse(response_line).or_fail()?;
                writeln!(output_writer, "{}", response.json).or_fail()?;
            }
        }

        output_writer.flush().or_fail()?;
        Ok(())
    }

    fn run_udp(self) -> orfail::Result<()> {
        const MAX_UDP_PACKET: usize = 65535;

        let socket = self.connect_to_server_udp().or_fail()?;

        let stdin = std::io::stdin();
        let input_reader = std::io::BufReader::new(stdin.lock());
        let stdout = std::io::stdout();
        let mut output_writer = std::io::BufWriter::new(stdout.lock());

        for line in input_reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;

            socket.send(request.json.text().as_bytes()).or_fail()?;

            if request.id.is_some() {
                let mut buf = vec![0u8; MAX_UDP_PACKET];
                let bytes_read = socket.recv(&mut buf).or_fail()?;
                (bytes_read > 0).or_fail_with(|()| {
                    "Faied to receive RPC response: unexpected EOF".to_owned()
                })?;
                let response_line = String::from_utf8(buf[..bytes_read].to_vec()).or_fail()?;
                let response = Response::parse(response_line).or_fail()?;
                writeln!(output_writer, "{}", response.json).or_fail()?;
            }
        }

        output_writer.flush().or_fail()?;
        Ok(())
    }

    fn connect_to_server(&self) -> orfail::Result<TcpStream> {
        let stream = TcpStream::connect(&self.server_addr.0)
            .or_fail_with(|e| format!("Failed to connect to '{}': {e}", self.server_addr.0))?;
        stream.set_nodelay(true).or_fail()?;
        Ok(stream)
    }

    fn connect_to_server_udp(&self) -> orfail::Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .or_fail_with(|e| format!("Failed to bind UDP socket: {e}"))?;
        socket
            .connect(&self.server_addr.0)
            .or_fail_with(|e| format!("Failed to connect to '{}': {e}", self.server_addr.0))?;
        Ok(socket)
    }
}
