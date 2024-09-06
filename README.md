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
  req              Generate a JSON-RPC request object JSON
  run-echo-server  Run a JSON-RPC echo server (for development or testing purposes)
  help             Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Examples
--------

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
