use std::{convert::Infallible, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerAddr(pub String);

impl FromStr for ServerAddr {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with(':') {
            Ok(Self(format!("127.0.0.1{s}")))
        } else {
            Ok(Self(s.to_owned()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct Request {
    pub json: nojson::RawJsonOwned,
    pub id: Option<RequestId>,
}

impl Request {
    pub fn parse(json_text: String) -> Result<Self, nojson::JsonParseError> {
        let json = nojson::RawJsonOwned::parse(json_text)?;
        let id = Self::validate_request_and_parse_id(json.value())?;
        Ok(Self { json, id })
    }

    fn validate_request_and_parse_id(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Option<RequestId>, nojson::JsonParseError> {
        if value.kind() == nojson::JsonValueKind::Array {
            return Err(value.invalid("batch requests are not supported"));
        }

        let mut has_jsonrpc = false;
        let mut has_method = false;
        let mut id = None;
        for (name, value) in value.to_object()? {
            match name.as_string_str()?.as_ref() {
                "jsonrpc" => {
                    if value.as_string_str()? != "2.0" {
                        return Err(value.invalid("jsonrpc version must be '2.0'"));
                    }
                    has_jsonrpc = true;
                }
                "id" => {
                    id = match value.kind() {
                        nojson::JsonValueKind::Integer => {
                            Some(RequestId::Number(value.try_into()?))
                        }
                        nojson::JsonValueKind::String => Some(RequestId::String(value.try_into()?)),
                        _ => {
                            // NOTE: null and float are rejected as the specification does not recommend those
                            return Err(value.invalid("id must be an integer or string"));
                        }
                    };
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
}

#[derive(Debug, Clone)]
pub struct Response {
    pub json: nojson::RawJsonOwned,
    pub id: Option<RequestId>,
}

impl Response {
    pub fn parse(json_text: String) -> Result<Self, nojson::JsonParseError> {
        let json = nojson::RawJsonOwned::parse(json_text)?;
        let id = Self::validate_response_and_parse_id(json.value())?;
        Ok(Self { json, id })
    }

    fn validate_response_and_parse_id(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Option<RequestId>, nojson::JsonParseError> {
        if value.kind() == nojson::JsonValueKind::Array {
            return Err(value.invalid("batch responses are not supported"));
        }

        let mut has_jsonrpc = false;
        let mut id = None;
        let mut has_result_or_error = false;

        for (name, value) in value.to_object()? {
            match name.as_string_str()?.as_ref() {
                "jsonrpc" => {
                    if value.as_string_str()? != "2.0" {
                        return Err(value.invalid("jsonrpc version must be '2.0'"));
                    }
                    has_jsonrpc = true;
                }
                "id" => {
                    id = match value.kind() {
                        nojson::JsonValueKind::Integer => {
                            Some(RequestId::Number(value.try_into()?))
                        }
                        nojson::JsonValueKind::String => Some(RequestId::String(value.try_into()?)),
                        _ => return Err(value.invalid("id must be an integer or string")),
                    };
                }
                "result" | "error" => {
                    has_result_or_error = true;
                }
                _ => {
                    // Ignore unknown members
                }
            }
        }

        if !has_jsonrpc {
            return Err(value.invalid("jsonrpc field is required"));
        }
        if !has_result_or_error {
            return Err(value.invalid("result or error field is required"));
        }

        Ok(id)
    }
}
