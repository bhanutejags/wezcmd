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

## Experimental TCP proxy

`wezcmd` includes a prototype supervised TCP tunnel mode without `ssh -fNL`.
Forwarding the wezcmd socket to a host grants that session this capability, so only forward it to trusted hosts.

One remote worker holds a control connection over the same forwarded Unix socket:

```bash
wezcmd proxy-worker --socket "$WEZCMD_SOCKET" \
  --session "$WEZCMD_SESSION_ID" --token "$WEZCMD_SESSION_TOKEN"
```

A remote shell can then ask the Mac daemon to listen locally and bridge each
incoming TCP connection back to the worker. Each TCP stream uses a fresh Unix
socket connection, so there is no custom byte-multiplexing protocol.

```bash
wezcmd proxy-listen --socket "$WEZCMD_SOCKET" \
  --session "$WEZCMD_SESSION_ID" --token "$WEZCMD_SESSION_TOKEN" \
  --local-port 8443 --remote-port 8443

wezcmd proxy-stop --socket "$WEZCMD_SOCKET" \
  --session "$WEZCMD_SESSION_ID" --token "$WEZCMD_SESSION_TOKEN" \
  --local-port 8443
```

If the worker/control connection exits, the daemon drops that session's listeners.
The existing `forward` command still uses `ssh -fNL`; this proxy is not wired into
dotfiles yet.

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
