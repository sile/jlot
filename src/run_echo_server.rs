use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpStream;

use orfail::OrFail;

use crate::types::ServerAddr;

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

    let listen_addr: ServerAddr = noargs::arg("<ADDR>")
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

fn run_server(listen_addr: ServerAddr) -> orfail::Result<()> {
    let listener = std::net::TcpListener::bind(listen_addr.0).or_fail()?;
    for incoming in listener.incoming() {
        let stream = incoming.or_fail()?;
        std::thread::spawn(move || {
            let _ = handle_client(stream);
        });
    }
    Ok(())
}

fn handle_client(stream: TcpStream) -> orfail::Result<()> {
    let reader = BufReader::new(stream.try_clone().or_fail()?);
    let mut writer = BufWriter::new(stream);
    for line in reader.lines() {
        let line = line.or_fail()?;
        nojson::RawJson::parse(&line)
            .and_then(|json| {
                let json_value = json.value();
                let Some(request_id) = parse_request(json_value)? else {
                    return Ok(Ok(()));
                };
                let response = nojson::object(|f| {
                    f.member("jsonrpc", "2.0")?;
                    f.member("id", request_id)?;
                    f.member("result", json_value)
                });
                Ok(writeln!(writer, "{response}"))
            })
            .unwrap_or_else(|e| {
                let response = nojson::object(|f| {
                    f.member("jsonrpc", "2.0")?;
                    f.member(
                        "error",
                        nojson::object(|f| {
                            // NOTE: For simplicity, we return a fixed error code (-32600) without an id field.
                            // In a production implementation, this should handle errors more granularly:
                            // - Parse errors should return -32700 without an id
                            // - Invalid requests should return -32600 with the id if present
                            f.member("code", -32600)?; // invalid-request code
                            f.member("message", e.to_string())
                        }),
                    )?;
                    f.member("id", ()) // null ID
                });
                writeln!(writer, "{response}")
            })
            .or_fail()?;
        writer.flush().or_fail()?;
    }
    Ok(())
}

fn parse_request<'text, 'raw>(
    value: nojson::RawJsonValue<'text, 'raw>,
) -> Result<Option<nojson::RawJsonValue<'text, 'raw>>, nojson::JsonParseError> {
    if value.kind() == nojson::JsonValueKind::Array {
        return Err(value.invalid("batch requests are not supported"));
    }

    let mut has_jsonrpc = false;
    let mut has_method = false;
    let mut id = None;
    for (name, value) in value.to_object()? {
        match name.to_unquoted_string_str()?.as_ref() {
            "jsonrpc" => {
                if value.to_unquoted_string_str()? != "2.0" {
                    return Err(value.invalid("jsonrpc version must be '2.0'"));
                }
                has_jsonrpc = true;
            }
            "id" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Integer | nojson::JsonValueKind::String
                ) {
                    return Err(value.invalid("id must be an integer or string"));
                }
                id = Some(value);
            }
            "method" => {
                if value.kind() != nojson::JsonValueKind::String {
                    return Err(value.invalid("method must be a string"));
                }
                has_method = true;
            }
            "params" => {
                if !matches!(
                    value.kind(),
                    nojson::JsonValueKind::Object | nojson::JsonValueKind::Array
                ) {
                    return Err(value.invalid("params must be an object or array"));
                }
            }
            _ => {
                // Ignore unknown members
            }
        }
    }

    if !has_jsonrpc {
        return Err(value.invalid("jsonrpc field is required"));
    }
    if !has_method {
        return Err(value.invalid("method field is required"));
    }

    Ok(id)
}
