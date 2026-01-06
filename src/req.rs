use std::num::NonZeroUsize;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("req")
        .doc("Generate a JSON-RPC request object JSON")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

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
    let params: Option<nojson::RawJsonOwned> = noargs::arg("[PARAMS]")
        .doc("Request parameters (JSON array or JSON object)")
        .take(args)
        .present_and_then(|a| {
            let json = nojson::RawJson::parse(a.value())?;
            if !matches!(
                json.value().kind(),
                nojson::JsonValueKind::Array | nojson::JsonValueKind::Object
            ) {
                return Err(json.value().invalid("must be a JSON array or JSON object"));
            }
            Ok(json.into_owned())
        })?;

    if args.metadata().help_mode {
        return Ok(false);
    }

    // Generate and output requests
    for id in 0..count.get() {
        let json = nojson::object(|f| {
            f.member("jsonrpc", "2.0")?;
            f.member("method", &method)?;
            if let Some(params) = &params {
                f.member("params", params)?;
            }
            if !notification {
                f.member("id", &id)?;
            }
            Ok(())
        });
        println!("{json}");
    }

    Ok(true)
}
