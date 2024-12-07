use clap::Parser;
use orfail::OrFail;

use jlot::{
    call::CallCommand, req::ReqCommand, run_echo_server::RunEchoServerCommand, stats::StatsCommand,
};

/// Command-line tool for JSON-RPC 2.0 over JSON Lines over TCP.
#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    Req(ReqCommand),
    Stats(StatsCommand),
    RunEchoServer(RunEchoServerCommand),
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::Req(c) => c.run().or_fail()?,
        Args::Stats(c) => c.run().or_fail()?,
        Args::RunEchoServer(c) => c.run().or_fail()?,
    }
    Ok(())
}
