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

$ jlot
Command-line tool for JSON-RPC 2.0 over JSON Lines over TCP

Usage: jlot <COMMAND>

Commands:
  call             Execute a JSON-RPC call
  stream-call      Execute a stream of JSON-RPC calls received from the standard input
  req              Generate a JSON-RPC request object JSON
  stats            Calculate statistics from JSON objects outputted by executing the command `stream-call --add-metadata ...`
  run-echo-server  Run a JSON-RPC echo server (for development or testing purposes)
  help             Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Examples
--------

### Basic RPC call

Start an echo server in a terminal:
```console
$ jlot run-echo-server 127.0.0.1:9000
```

Execute an RPC call in another terminal:
```console
$ jlot call 127.0.0.1:9000 $(jlot req hello '["world"]' --id 2) | jq .
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
$ jlot run-echo-server 127.0.0.1:9000
```

Execute 1000 RPC calls with pipelining enabled and gather the statistics:
```console
$ jlot req put --id 0 --count 1000 | \
    jlot stream-call 127.0.0.1:9000 --pipelining 10 --add-metadata | \
    jlot stats | \
    jq .
{
  "duration": 0.378478,
  "max_concurrency": 10,
  "count": {
    "calls": 1000,
    "batch_calls": 0,
    "missing_metadata_calls": 0,
    "requests": 1000,
    "responses": {
      "ok": 1000,
      "error": 0
    }
  },
  "rps": 2642.1614994794945,
  "bps": {
    "outgoing": 864303.8697097321,
    "incoming": 1622921.2794402845
  },
  "latency": {
    "min": 9.525e-05,
    "p25": 0.000223541,
    "p50": 0.0003005,
    "p75": 0.010511875,
    "max": 0.012503042,
    "avg": 0.003430977
  }
}
```
