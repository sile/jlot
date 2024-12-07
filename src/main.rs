use clap::Parser;
use orfail::OrFail;

use jlot::{
    req::ReqCommand, run_echo_server::RunEchoServerCommand, stats::StatsCommand,
    stream_call::StreamCallCommand,
};

/// Command-line tool for JSON-RPC 2.0 over JSON Lines over TCP.
#[derive(Parser)]
#[clap(version)]
enum Args {
    StreamCall(StreamCallCommand),
    Req(ReqCommand),
    Stats(StatsCommand),
    RunEchoServer(RunEchoServerCommand),
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::StreamCall(c) => c.run().or_fail()?,
        Args::Req(c) => c.run().or_fail()?,
        Args::Stats(c) => c.run().or_fail()?,
        Args::RunEchoServer(c) => c.run().or_fail()?,
    }
    Ok(())
}
