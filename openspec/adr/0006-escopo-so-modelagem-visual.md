---
status: superseded by 0009 (parte: sem conexão com banco)
---

# Scope: visual modeling only

The product does not connect to or read live databases and does not execute SQL (export, import, or migrations). Diagram exports are PNG and SVG. Targets: Windows 10/11, Ubuntu 22.04/24.04, Fedora, and macOS.

## Consequences

Smaller stack (no `sqlparser`/`sqlx`); the type catalog is generic rather than tied to a single SQL dialect; engineering investment goes into the canvas, bidirectional editing, and export quality.
