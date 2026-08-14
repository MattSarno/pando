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

| Table          | Purpose                                                             | Key fields                                                                  |
|----------------|----------------------------------------------------------------------|------------------------------------------------------------------------------|
| subjects       | Anchors for projects, ongoing focuses, and relationships             | slug, title, category, status, summary, updated_at, updated_by              |
| memory_entries | Durable context the AI reads to stay consistent, with per-subject overrides | id, scope, key, subject_id, description, content, updated_at, updated_by |
| tasks          | Cross-surface action items                                          | id, text, status, priority, linked_subject, due_date, source, created_at, completed_at, updated_at, updated_by |

All three tables are finalized — see `migrations/0001_create_subjects.up.sql`,
`migrations/0002_create_memory_entries.up.sql`, and
`migrations/0003_create_tasks.up.sql` for the real DDL.
`migrations/0004_restrict_memory_entries_subject_delete.up.sql` later changes
`memory_entries.subject_id`'s FK from `ON DELETE CASCADE` to
`ON DELETE RESTRICT` — see `subject_delete` in #6 for why.

Recurring/habit-style tasks were deliberately scoped out of `tasks` — planned
as a separate `habits` + `habit_completions` pair later, since a recurring
definition with a completion history is a different shape than a one-off
task.

Replaces the original draft's separate `profile`, `areas`, `topics`, and
`people` tables — those are now `memory_entries` rows (optionally scoped to
a `subjects` row via `subject_id`) rather than distinct tables.

## 6. MCP Tool Contract

Every client authenticates with the same credential (see #8), so the server
can't tell clients apart from auth alone. Instead, every mutating tool takes
an explicit identity param — each client's own config tells it the fixed
string to pass (e.g. `"claude-code"`, `"cowork"`, `"claude-chat"` today).
This is intentionally unconstrained/unvalidated (no allowlist, no DB
`CHECK`) — enforcing a known set of clients would work against #1's
"swapping to the next AI should be seamless" goal, since a new agent or tool
would need a schema/code change before it could write anything.

`tasks` is the only table with both `source` (who created it) and
`updated_by` (who last touched it) as separate columns, so `task_create`
takes `source` while every other mutation takes `updated_by`. `subjects`
and `memory_entries` have no `source` column at all — `updated_by` is used
on both create and update for those two, since there's less value in
remembering who first wrote a living fact/note versus who last confirmed
it, unlike a task's provenance.

Nullable fields on `*_update` tools (`subject.summary`, `task.due_date`) are
cleared by passing an empty string `""` — omitting the param entirely means
"leave it alone." An LLM caller can't be relied on to reliably distinguish
"omit this key" from "pass JSON `null`" in generated tool calls, so real
`null` isn't used as the clear signal.

`memory_entries.key` must match `^[a-z0-9]+(-[a-z0-9]+)*$` (lowercase,
hyphenated) — same shape and same reasoning as `subjects.slug`: cheap
insurance against casing/spacing drift (`"tz"` vs `"timezone"`) creating
duplicate, unrelated entries for what's meant to be the same concept.
Enforced wherever `key` is written (`memory_write`).

**Subjects**
- `subject_create(slug, title, category, summary?, updated_by)` → created row.
  - `slug` is client-chosen, not server-generated — must match
    `^[a-z0-9]+(-[a-z0-9]+)*$` (lowercase, hyphenated), validated at the tool
    layer since Postgres only enforces uniqueness, not shape. Permanent once
    set — never renameable, since `memory_entries`/`tasks` FK into it.
  - `status` is not a param — it's set server-side, not client-controlled.
    `'active'` for `category` in (`project`, `focus`); `NULL` for
    `relationship` (per the `status_matches_category` check). There's no
    real use case for creating a subject already paused/finished; use
    `subject_update` for that after creation.
  - Duplicate `slug` is a hard error, not an upsert — use `subject_update`
    to change an existing subject.
- `subject_list(category?, status?)` → matching rows, ordered `updated_at
  desc` (whatever you're actively working on surfaces first — same ordering
  as `task_list`).
- `subject_update(slug, title?, status?, summary?, updated_by)` → updated row.
  - `category` is immutable after creation — it determines which `status`
    values are legal, so changing it would require re-validating status too;
    not needed yet.
  - `slug` itself is not renameable — it's referenced by FK from
    `memory_entries` and `tasks`.
  - Validating a new `status` against the row's (unchanged) `category`
    requires reading the current row first, rather than relying on the DB
    check to reject it blind.
  - At least one of `title` / `status` / `summary` must be provided — a
    call with none of them is rejected rather than allowed as a silent
    no-op that only bumps `updated_at`/`updated_by`.
- `subject_delete(slug, updated_by)` → void.
  - In one transaction: deletes every `memory_entries` row scoped to it,
    then the `subjects` row itself. Any `tasks` linked to it survive with
    `linked_subject` set to `NULL` (existing `ON DELETE SET NULL`, no
    explicit step needed). No `cascade_memory` opt-in/failure mode — taking
    the scoped memory with it is the expected behavior of deleting a
    subject, not something worth gating behind a param nobody would leave
    off.
  - `migrations/0004` makes the memory FK `ON DELETE RESTRICT` specifically
    so nothing *outside* this tool can silently cascade or orphan that
    data — this transaction is the one deliberate, documented place that's
    allowed to.
  - `updated_by` has nowhere to persist to once the row's gone, but is kept
    and logged (see #8's logging line) so there's still a record of who
    deleted it.

**Memory**
- `memory_read(scope, key, subject_id?)` → matching row (`content`,
  `description`, `updated_at`, `updated_by`), or not-found (empty result,
  not an error — "no memory of X yet" is a normal case).
  - If `subject_id` is given but no override row exists for it, falls back
    to the global default (`subject_id IS NULL`) for that `(scope, key)`.
    Returns only the resolved value, not both override and default.
- `memory_write(scope, key, content, description?, subject_id?, updated_by)`
  → upserted row. Upserts on `(scope, key)` when `subject_id` is omitted,
  `(scope, key, subject_id)` otherwise — matching the two unique indexes.
  - `key` must match the kebab-case pattern (see the shared convention
    note above) — this is the tool that actually enforces it, since it's
    the only place `key` gets written.
- `memory_delete(scope, key, subject_id?, updated_by)` → void.
  - Targets the exact row like `memory_write`'s upsert target — no
    fallback like `memory_read` has, so deleting an override can never
    accidentally take out the global default or vice versa.
  - Errors if no row matches the target — a missing target most likely
    means a wrong `key`, and silently succeeding would hide that.
  - `updated_by` logged same as `subject_delete`, nothing to persist it to.
  - `description` is required on first write for a key — a brand-new entry
    with no `description` is a hard error, no placeholder, so every entry
    stays easy to find via `memory_search` later. Optional on later writes
    to the same key, where omitted means "keep the existing description,"
    so correcting `content` doesn't force resupplying it.
- `memory_search(query, scope?, subject_id?)` → matching rows. Matches
  `query` against `key`, `description`, and `content` via `ILIKE`
  (full-text search is more than this needs at personal scale).
  `subject_id`, when given, is a strict equality filter — only that
  subject's own override rows, not global defaults folded in. Unlike
  `memory_read`, no fallback here.
- `memory_list(scope?, subject_id?)` → matching rows, ordered `updated_at
  desc` (same ordering as `subject_list`/`task_list`). No text match, kept
  as a separate tool from `memory_search` rather than making `query`
  optional there — browsing and searching are different intents even
  though the underlying filters overlap. `subject_id` is the same strict
  equality filter as `memory_search`.

**Tasks**
- `task_create(text, source, priority?, due_date?, linked_subject?)` →
  created row. `status` always starts `'open'`; `source` also seeds the
  initial `updated_by`.
- `task_update(id, updated_by, status?, priority?, due_date?, text?)` →
  updated row.
  - `done` is a terminal state. `status` here only accepts `'open'` /
    `'in_progress'`, and only while the task isn't already `done` — reaching
    `done` is exclusively through `task_complete`, and there's no path back
    out of it. A task marked done by mistake, or needing more work, gets a
    new `task_create` rather than being reopened. This also means
    `completed_at` never needs to be touched here — it's set once by
    `task_complete` and never cleared.
  - `linked_subject` is fixed at creation and not editable here — same
    "not renameable/relinkable" reasoning as `subjects.slug`.
  - At least one of `status` / `priority` / `due_date` / `text` must be
    provided — same no-op rejection rule as `subject_update`.
- `task_complete(id, updated_by)` → updated row. Sets `status = 'done'` and
  `completed_at = now()`. Errors if the task is already `done` — same
  reasoning as `memory_delete`: calling it on something already in the
  target state most likely means the caller has stale info (e.g. two
  clients racing, or forgot it already completed this), and silently
  succeeding would hide that.
- `task_delete(id, updated_by)` → void.
  - Errors if `id` doesn't exist. Nothing else FKs into `tasks.id`, so
    unlike `subject_delete` there's no cascade to worry about.
  - For fixing mistakes (junk/duplicate/accidentally-created tasks) — not
    an alternative to `task_complete`, which already covers anything
    genuinely finished and preserves it as history.
- `task_list(status?, stale_days?, linked_subject?)` → matching rows,
  ordered `updated_at desc`. `stale_days` is independent of `status` — it
  only means `updated_at` older than `now() - stale_days`, ANDed with
  whatever `status`/`linked_subject` filters are also given. Want old open
  work? Pass `status: 'open', stale_days: 30`. Want old completed tasks for
  archival? Pass `status: 'done', stale_days: 90`. Composable rather than
  baking in "not done," which also avoids the `status: 'done'` +
  `stale_days` contradiction a baked-in version would hit.

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

- Hosting targets (Fly.io vs VPS)
- Auth mechanism specifics (API key vs OAuth)

## 10. Repo / Licensing
- Repo name: `pando`
- Visibility: public
- License: AGPL
- Nothing environment-specific or personal (`.env`, seed data, tokens) is ever committed. See `.gitignore` before first commit.