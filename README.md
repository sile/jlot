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
  call             Execute a stream of JSON-RPC calls received from the standard input
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

Start an echo server in a terminal (":9000" is shorthand for "127.0.0.1:9000"):
```console
$ jlot run-echo-server :9000
```

Execute an RPC call in another terminal:
```console
$ jlot req hello '["world"]' --id 2 | jlot call :9000 | jq .
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
$ jlot run-echo-server :9000
```

Execute 1000 RPC calls with pipelining enabled and gather the statistics:
```console
$ jlot req put --count 100000 | \
    jlot call :9000 --concurrency 10 --add-metadata --buffer-input | \
    jlot stats | \
    jq .
{
  "rpc_calls": 100000,
  "duration": 0.608289541,
  "max_concurrency": 10,
  "rps": 164395.39604051816,
  "latency": {
    "min": 0.000016416,
    "p25": 0.0000435,
    "p50": 0.000057625,
    "p75": 0.000074584,
    "max": 0.000302208,
    "avg": 0.000060041
  }
}
```
