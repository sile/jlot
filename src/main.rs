use clap::Parser;
use orfail::OrFail;

use jlot::{
    call::CallCommand, req::ReqCommand, run_echo_server::RunEchoServerCommand,
    stream_call::StreamCallCommand,
};

/// Command-line tool for JSON-RPC 2.0 over JSON Lines over TCP.
#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    StreamCall(StreamCallCommand),
    Req(ReqCommand),
    RunEchoServer(RunEchoServerCommand),
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::StreamCall(c) => c.run().or_fail()?,
        Args::Req(c) => c.run().or_fail()?,
        Args::RunEchoServer(c) => c.run().or_fail()?,
    }
    Ok(())
}
