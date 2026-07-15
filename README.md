# wezcmd

`wezcmd` lets a remote SSH session ask the local Mac to perform a small, whitelisted set of actions through a reverse-forwarded Unix socket.

It exists because terminal escape passthrough is unreliable inside multiplexers like Zellij. The protocol is intentionally tiny: one newline-delimited JSON request per connection.

## Commands

```bash
wezcmd daemon --socket ~/.wezcmd/wezcmd.sock
wezcmd probe /tmp/wezcmd-host.123.sock
wezcmd send --socket /tmp/wezcmd-host.123.sock open --url https://example.com
wezcmd send --socket /tmp/wezcmd-host.123.sock notify --title Build --body done
wezcmd send --socket /tmp/wezcmd-host.123.sock vscode --path /home/me/src --host cd
wezcmd send --socket /tmp/wezcmd-host.123.sock forward --port 8443 --host cd
```

## Protocol

```json
{"cmd":"open","url":"https://example.com"}
{"cmd":"notify","title":"Build","body":"done"}
{"cmd":"vscode","path":"/abs/path","host":"my-host"}
{"cmd":"forward","port":8443,"host":"my-host"}
```

Replies are `{"ok":true}` or `{"ok":false,"err":"..."}`.

## Development

```bash
cargo fmt
cargo test
./scripts/build-release.sh
```

Release assets are built for:

- `aarch64-apple-darwin` (Apple Silicon Macs only)
- `aarch64-unknown-linux-musl`
- `x86_64-unknown-linux-musl`
