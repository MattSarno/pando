# Pando

A shared memory backend for my AI personal assistant of choice. One source of truth instead of multiple separate memories. A brain one might say?

## How it Works 

Pando exposes a personal context store (projects, tasks, preferences, notes) as an MCP server, so any connected AI surface can read and write the same state.

See [`design.md`](./design.md) for the full architecture.

## Status

🌱 Early design phase — not yet functional.

## Stack

- Rust / Axum
- Postgres (Neon)
- MCP
