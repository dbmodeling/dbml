# DB Modeling for VS Code

Official **DB Modeling** extension for Visual Studio Code. Provides visual, bidirectional relational database modeling powered by **DBML (Database Markup Language)**.

## Features

- **Interactive Canvas:** Render tables, columns, primary/foreign keys, groups, and orthogonal relationships.
- **Real-Time Bidirectional Sync:** Text edits update the canvas immediately, and visual edits generate minimal textual patches in your `.dbml` document.
- **Unified Undo/Redo:** Fully integrated with VS Code's native Undo/Redo stack (`Ctrl+Z` / `Ctrl+Y`).
- **Deterministic Layout Sidecar:** Preserves visual positions and colors in `<schema>.dbml.layout.json`.

## How to Use

1. Open any `.dbml` file in VS Code.
2. Click the canvas icon in the editor title bar or run `DB Modeling: Open in Visual Canvas` from the Command Palette (`Ctrl+Shift+P`).

## Settings

- `dbmodeling.defaultEditor` (`text` | `canvas`, default `text`): which editor opens `.dbml` files. The canvas is always one click away from the editor title bar. Source Control diffs and merge conflicts stay in the text editor.
- `dbmodeling.cliPath`: path to the `dbml` CLI used by **Connect to Database**. It is only read from your user settings (a workspace cannot set it), a path other than `dbml` asks for confirmation before first use, and the CLI must be version 0.1.4 or newer.

## Trust and virtual workspaces

In untrusted workspaces the database connection (it runs the `dbml` CLI) is disabled; the canvas and the DBML editor keep working. In virtual workspaces and VS Code for the Web (vscode.dev) the canvas and editing work too, but **Connect to Database** is unavailable.

## Development

```sh
cd vscode-extension
npm install
npm test          # vitest, with a lightweight mock of the `vscode` module
npm run compile   # type-check
```
