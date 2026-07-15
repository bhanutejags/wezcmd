# wezcmd

Small Rust replacement for the dotfiles `wezcmd` helper: a local Unix-socket daemon plus a tiny client for remote SSH sessions.

## Commands

```bash
cargo fmt
cargo test
```

## Protocol

- Newline-delimited JSON over a Unix socket.
- One request per connection.
- Keep it boring: no gRPC, HTTP, Smithy, or service framework.

## Security

- Validate requests before dispatch.
- Subprocesses are argv-only.
- No shell dispatch (`sh -c`, `bash -c`, etc.).
