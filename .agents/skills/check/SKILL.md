---
name: check
description: Roda localmente as mesmas verificações do CI (fmt, clippy, testes Rust e testes do canvas-web) e resume as falhas. Use antes de commit/PR ou quando o usuário pedir "/check".
---
# check

Rode na raiz do repositório, nesta ordem, e **não pare na primeira falha**: rode todos e só então resuma.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd packages/canvas-web && npm test)
```

Responda em pt-BR com uma linha por passo (✅/❌). Para cada ❌, mostre o erro relevante (arquivo:linha + mensagem), não o log inteiro.

Não corrija nada sem o usuário pedir. Exceção: se só o `fmt` falhar, ofereça rodar `cargo fmt --all`.
