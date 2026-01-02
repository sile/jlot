/*
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
*/

fn main() -> noargs::Result<()> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
    args.metadata_mut().app_description = env!("CARGO_PKG_DESCRIPTION");

    if noargs::VERSION_FLAG.take(&mut args).is_present() {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    noargs::HELP_FLAG.take_help(&mut args);

    if let Some(help) = args.finish()? {
        print!("{}", help);
        return Ok(());
    }

    Ok(())
    /*
        let args = Args::parse();
        match args {
            Args::Call(c) => c.run().or_fail()?,
            Args::Req(c) => c.run().or_fail()?,
            Args::Stats(c) => c.run().or_fail()?,
            Args::RunEchoServer(c) => c.run().or_fail()?,
        }
        Ok(())
    */
}
