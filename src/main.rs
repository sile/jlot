use clap::Parser;
use orfail::OrFail;

use jlot::{call::CallCommand, echo_server::EchoServerCommand, req::ReqCommand};

#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    Req(ReqCommand),
    EchoServer(EchoServerCommand), // TODO: bench
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::Req(c) => c.run().or_fail()?,
        Args::EchoServer(c) => c.run().or_fail()?,
    }
    Ok(())
}
