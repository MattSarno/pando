# Pando Design Document

## 1. Purpose

Pando is a shared memory personal-assistant backend. It exists so that Claude Code, Claude Cowork, and Claude chat all read and write the same context decisions, project state, and action items instead of each holding their own isolated memory.

The core issue this solves: finishing something in one Claude surface and having to re-explain it in another. Also swapping away from claude to the next AI should be "seamless".

## 2. Non-Goals

## 3. Architecture Overview

## 4. Tech Stack

## 5. Schema

## 6. MCP Tool Contract

## 7. Build Order

## 8. Security

## 9. Open Questions

## 10. Repo / Licensing
- Repo name: `pando`
- Visibility: public
- License: AGPL
- Nothing environment-specific or personal (`.env`, seed data, tokens) is ever committed. See `.gitignore` before first commit.