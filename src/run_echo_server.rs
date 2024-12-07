use std::net::TcpStream;

use jsonlrpc::{
    ErrorCode, ErrorObject, JsonRpcVersion, JsonlStream, MaybeBatch, RequestObject, ResponseObject,
};
use orfail::OrFail;

use crate::types::ServerAddr;

/// Run a JSON-RPC echo server (for development or testing purposes).
///
/// This server will respond to every request with a response containing
/// the same request object as the result value.
#[derive(Debug, clap::Args)]
pub struct RunEchoServerCommand {
    /// Listen address.
    listen_addr: ServerAddr,
}

impl RunEchoServerCommand {
    pub fn run(self) -> orfail::Result<()> {
        let listener = std::net::TcpListener::bind(self.listen_addr.0).or_fail()?;
        for incoming in listener.incoming() {
            let stream = incoming.or_fail()?;
            std::thread::spawn(move || {
                let _ = handle_client(stream);
            });
        }
        Ok(())
    }
}

fn handle_client(stream: TcpStream) -> orfail::Result<()> {
    let mut stream = JsonlStream::new(stream);
    loop {
        let response = match stream.read_value::<MaybeBatch<RequestObject>>() {
            Ok(MaybeBatch::Single(request)) => echo_response(request).map(MaybeBatch::Single),
            Ok(MaybeBatch::Batch(requests)) => {
                let responses = requests
                    .into_iter()
                    .filter_map(echo_response)
                    .collect::<Vec<_>>();
                if responses.is_empty() {
                    None
                } else {
                    Some(MaybeBatch::Batch(responses))
                }
            }
            Err(e) if e.is_io() => {
                break;
            }
            Err(e) => Some(MaybeBatch::Single(ResponseObject::Err {
                jsonrpc: JsonRpcVersion::V2,
                id: None,
                error: ErrorObject {
                    code: ErrorCode::guess(&e),
                    message: format!(
                        "[{} ERROR] {e}",
                        format!("{:?}", e.classify()).to_uppercase()
                    ),
                    data: None,
                },
            })),
        };

        if let Some(response) = response {
            stream.write_value(&response).or_fail()?;
        }
    }

    Ok(())
}

fn echo_response(request: RequestObject) -> Option<ResponseObject> {
    request.id.clone().map(|id| ResponseObject::Ok {
        jsonrpc: JsonRpcVersion::V2,
        id,
        result: serde_json::to_value(&request).expect("unreachable"),
    })
}
