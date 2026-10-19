# Pando

A shared memory backend for my AI personal assistant of choice. One source of truth instead of multiple separate memories. A brain one might say?

## How it Works 

Pando exposes a personal context store (projects, tasks, preferences, notes) as an MCP server, so any connected AI surface can read and write the same state.

See [`design.md`](DESIGN.md) for the full architecture.

## Status

🌱 Live and in daily use with Claude Code, Claude chat, and Cowork. Still pre-v1.

## Stack

- Rust / Axum
- Postgres (Neon)
- MCP
