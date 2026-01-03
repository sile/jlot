use std::io::{BufRead, BufReader};
use std::net::TcpStream;

use jsonlrpc::{
    ErrorCode, ErrorObject, JsonRpcVersion, JsonlStream, RequestObject, ResponseObject,
};
use orfail::OrFail;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("run-echo-server")
        .doc(concat!(
            "Run a JSON-RPC echo server (for development or testing purposes)\n",
            "\n",
            "This server will respond to every request with a response containing\n",
            "the same request object as the result value."
        ))
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let listen_addr: std::net::SocketAddr = noargs::arg("<ADDR>")
        .doc("Listen address")
        .example("127.0.0.1:8080")
        .take(args)
        .then(|a| a.value().parse())?;

    if args.metadata().help_mode {
        return Ok(false);
    }

    run_server(listen_addr)?;
    Ok(true)
}

fn run_server(listen_addr: std::net::SocketAddr) -> orfail::Result<()> {
    let listener = std::net::TcpListener::bind(listen_addr).or_fail()?;
    for incoming in listener.incoming() {
        let stream = incoming.or_fail()?;
        std::thread::spawn(move || {
            let _ = handle_client(stream);
        });
    }
    Ok(())
}

fn handle_client2(stream: TcpStream) -> orfail::Result<()> {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line.or_fail()?;
        let json = nojson::RawJson::parse(&line).or_fail()?;
        let request_id = parse_request(json.value()).or_fail()?;
    }
    Ok(())
}

fn parse_request<'text, 'raw>(
    value: nojson::RawJsonValue<'text, 'raw>,
) -> Result<Option<nojson::RawJsonValue<'text, 'raw>>, nojson::JsonParseError> {
    if value.kind() == nojson::JsonValueKind::Array {
        return Err(value.invalid("todo")); // Batch requests are not supported
    }

    let mut has_jsonrpc = false;
    let mut has_method = false;
    let mut id = None;
    for (name, value) in value.to_object()? {
        match name.to_unquoted_string_str()?.as_ref() {
            "jsonrpc" => {
                if value.to_unquoted_string_str()? != "2.0" {
                    return Err(value.invalid("todo"));
                }
                has_jsonrpc = true;
            }
            "id" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Integer | nojson::JsonValueKind::String
                ) {
                    return Err(value.invalid("todo"));
                }
                id = Some(value);
            }
            "method" => {
                if value.kind() != nojson::JsonValueKind::String {
                    return Err(value.invalid("todo"));
                }
                has_method = true;
            }
            "params" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Object | nojson::JsonValueKind::Array
                ) {
                    return Err(value.invalid("todo"));
                }
            }
            _ => {
                // Ignore unknown members
            }
        }
    }

    if !has_jsonrpc {
        return Err(value.invalid("todo"));
    }
    if !has_method {
        return Err(value.invalid("todo"));
    }

    Ok(id)
}

fn handle_client(stream: TcpStream) -> orfail::Result<()> {
    let mut stream = JsonlStream::new(stream);
    loop {
        let response = match stream.read_value::<RequestObject>() {
            Ok(request) => echo_response(request),
            Err(e) if e.is_io() => {
                break;
            }
            Err(e) => Some(ResponseObject::Err {
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
            }),
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
