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
