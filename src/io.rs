pub fn maybe_eos<T>(result: serde_json::Result<T>) -> serde_json::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.io_error_kind() == Some(std::io::ErrorKind::UnexpectedEof) => Ok(None),
        Err(e) => Err(e),
    }
}
