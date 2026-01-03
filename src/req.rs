use std::num::NonZeroUsize;

use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams};
use orfail::OrFail;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("req")
        .doc("Generate a JSON-RPC request object JSON")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let id: RequestId = noargs::opt("id")
        .short('i')
        .ty("INTEGER | STRING")
        .doc("Request ID")
        .default("0")
        .take(args)
        .then(|o| -> Result<RequestId, std::convert::Infallible> {
            let val = o.value();
            val.parse::<i64>()
                .map(RequestId::Number)
                .or_else(|_| Ok(RequestId::String(val.to_string())))
        })?;
    let notification: bool = noargs::flag("notification")
        .short('n')
        .doc("Exclude the \"id\" field from the resulting JSON object")
        .take(args)
        .is_present();
    let count: NonZeroUsize = noargs::opt("count")
        .short('c')
        .ty("INTEGER")
        .doc("Count of requests to generate")
        .default("1")
        .take(args)
        .then(|o| o.value().parse())?;
    let method: String = noargs::arg("<METHOD>")
        .doc("Method name")
        .example("GetFoo")
        .take(args)
        .then(|a| a.value().parse())?;
    let params: Option<RequestParams> = noargs::arg("[PARAMS]")
        .doc("Request parameters (JSON array or JSON object)")
        .take(args)
        .present_and_then(|a| {
            let json_str = a.value();
            serde_json::from_str(json_str).map_err(|e| format!("invalid JSON: {}", e))
        })?;

    if args.metadata().help_mode {
        return Ok(false);
    }

    // Generate and output requests
    let request = RequestObject {
        jsonrpc: JsonRpcVersion::V2,
        method,
        params,
        id: (!notification).then_some(id),
    };

    let json = serde_json::to_string(&request).or_fail()?;
    for _ in 0..count.get() {
        println!("{json}");
    }

    Ok(true)
}
