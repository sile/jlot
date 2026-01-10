jlot
====

[![jlot](https://img.shields.io/crates/v/jlot.svg)](https://crates.io/crates/jlot)
[![Documentation](https://docs.rs/jlot/badge.svg)](https://docs.rs/jlot)
[![Actions Status](https://github.com/sile/jlot/workflows/CI/badge.svg)](https://github.com/sile/jlot/actions)
![License](https://img.shields.io/crates/l/jlot)

This is a command-line tool for [JSON-RPC 2.0] over [JSON Lines] over TCP.

[JSON-RPC 2.0]: https://www.jsonrpc.org/specification
[JSON Lines]: https://jsonlines.org/

```console
$ cargo install jlot

$ jlot -h
Command-line tool for JSON-RPC 2.0 over JSON Lines over TCP

Usage: jlot [OPTIONS] <COMMAND>

Commands:
  req         Generate a JSON-RPC request object JSON
  call        Read JSON-RPC requests from standard input and execute the RPC calls
  bench       Run JSON-RPC benchmark
  stats       Calculate statistics from JSON objects outputted by the bench command
  echo-server Run a JSON-RPC echo server (for development or testing purposes)

Options:
      --version Print version
  -h, --help    Print help ('--help' for full help, '-h' for summary)
```

Examples
--------

### Basic RPC call

Start an echo server in a terminal (":9000" is shorthand for "127.0.0.1:9000"):
```console
$ jlot echo-server :9000
```

Execute an RPC call in another terminal:
```console
$ jlot req hello --params '["world"]' | jlot call :9000 | jq .
{
  "jsonrpc": "2.0",
  "result": {
    "id": 2,
    "jsonrpc": "2.0",
    "method": "hello",
    "params": [
      "world"
    ]
  },
  "id": 2
}
```

### Benchmarking

Start an echo server in a terminal:
```console
$ jlot echo-server :9000
```

Execute 100,000 RPC calls in benchmarking mode and gather the statistics:
```console
$ jlot req put --count 100000 | \
    jlot bench :9000 --concurrency 10 | \
    jlot stats
{
  "elapsed_seconds": 0.356318,
  "requests_per_second": 280648,
  "avg_latency_seconds": 0.000034634,
  "detail": {
    "count": { "success": 100000, "error": 0 },
    "size": { "request_avg_bytes": 43, "response_avg_bytes": 81 },
    "latency": { "min": 0.000013, "p25": 0.000024, "p50": 0.000028, "p75": 0.000035, "max": 0.038994 },
    "concurrency": { "max": 10 }
  }
}
```
