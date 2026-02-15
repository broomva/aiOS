# aiosd

Demo daemon for local `aiOS` kernel execution.

## Purpose

Run a bootstrap session and emit the event stream while exercising core tools.

## Run

```bash
cargo run -p aiosd -- --root .aios
```

## What It Demonstrates

1. Session creation
2. Tick lifecycle execution
3. Tool dispatch (`fs.write`, `shell.exec`, `fs.read`)
4. Checkpoint + heartbeat emission
