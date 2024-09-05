use clap::Parser;
use orfail::OrFail;

use jlot::{call::CallCommand, req::ReqCommand, run_echo_server::RunEchoServerCommand};

#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    Req(ReqCommand),
    RunEchoServer(RunEchoServerCommand),
    // TODO: bench command
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::Req(c) => c.run().or_fail()?,
        Args::RunEchoServer(c) => c.run().or_fail()?,
    }
    Ok(())
}
