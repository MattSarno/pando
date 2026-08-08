# Pando Design Document

## 1. Purpose

Pando is a shared memory personal-assistant backend. It exists so that Claude Code, Claude Cowork, and Claude chat all read and write the same context decisions, project state, and action items instead of each holding their own isolated memory.

The core issue this solves: finishing something in one Claude surface and having to re-explain it in another. Also swapping away from claude to the next AI should be "seamless".

## 2. Non-Goals

- Not a public-facing product. Single user, starting with three trusted clients.
- Not optimized for massive scale, optimize for correctness over throughput.
- Not replacing per-project technical docs, READMEs, or code comments, Pando holds cross-cutting personal/project context, not code itself.

## 3. Architecture Overview

    Claude Code  ─┐
    Claude Chat  ─┼──▶  Pando (Axum, MCP server)  ──▶  Postgres (Neon?)
    Cowork       ─┘

All three clients call the same MCP tools against the same database. No sync, no reconciliation, last write wins (should only be using one at a time any way), and every client reads current state on every call.

## 4. Tech Stack

- **Language/framework:** Rust, Axum
- **Database:** Postgres, hosted on Neon?
- **Interface:** MCP server (tools, not REST — see #6)
- **Deployment:** containerized, TBD host (Fly.io / small VPS)

## 5. Schema

| Table       | Purpose                                | Key fields (draft)                       |
|-------------|----------------------------------------|------------------------------------------|
| profile     | Stable identity facts                  | key, value, updated_at                   |
| areas       | Active projects, keyed by slug         | slug, title, status, context, updated_at |
| topics      | Durable patterns/preferences by domain | domain, key, value                       |
| people      | Relationship context                   | name, relationship, notes                |
| tasks       | Cross-surface action items             | id, source, status, linked_area, text    |
| preferences | Behavioral preferences for Claude      | key, value                               |

(This is a draft shape, not final DDL — flesh out per-table before writing
migrations.)

## 6. MCP Tool Contract

Draft tool list — exact names/params to be finalized before implementation:
- `memory_read(scope, key)`
- `memory_write(scope, key, value)`
- `memory_search(query)`
- `task_create(text, source, linked_area?)`
- `task_complete(id)`
- `task_list(status?, stale_days?)`

## 7. Build Order

1. Finalize schema + MCP tool contract (this doc)
2. Bare Axum + Postgres service, no auth, running locally
3. Wire up one client end-to-end (Claude Code)
4. Add Cowork, then chat as a connector
5. Security hardening (see #8) — last, once shape is proven

## 8. Security

- **No credentials/secrets ever stored** — categorical rule, no exceptions, regardless of other protections in place.
- Chat requires the MCP endpoint to be reachable over the public internet. Full network isolation is not viable for the whole system.
- **Primary defense:** strong auth (API key or OAuth) on the endpoint.
- **Secondary:** rate limiting + logging, so a leaked/guessed token doesn't mean unlimited silent access.
- Categorical content exclusion (no sensitive/confidential info written into memory at all) is the backstop, not the only layer.

## 9. Open Questions

- Exact field-level schema / migrations
- Final MCP tool list and parameter shapes
- Hosting targets (Fly.io vs VPS)
- Auth mechanism specifics (API key vs OAuth)

## 10. Repo / Licensing
- Repo name: `pando`
- Visibility: public
- License: AGPL
- Nothing environment-specific or personal (`.env`, seed data, tokens) is ever committed. See `.gitignore` before first commit.