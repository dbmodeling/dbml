# Benchmarks & WebAssembly Performance (#116, #117, #129)

Deterministic performance measurements for the DBML pipeline (`parse`, `analyze`, `lint`, `layout`, `route`) and the `dbml-wasm` module.

## Reproducing

```bash
# Native Rust pipeline benchmark (50 / 200 / 500 tables)
cargo run --release -p dbml-wasm --example bench_pipeline

# WebAssembly benchmark + 200-frame drag linear memory check
node scripts/bench-wasm.mjs
```

## 1. Native Rust Pipeline (#116)

Measured in release mode on deterministic schemas with FK chains, groups, and enums (`crates/dbml-wasm/examples/bench_pipeline.rs`):

| Tables | `parse` (ms) | `analyze` (ms) | `lint` (ms) | `layout` (ms) | `route` (ms) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 50 | 0.11 | 0.28 | 0.90 | 0.01 | 0.21 |
| 200 | 0.41 | 1.08 | 3.40 | 0.07 | 5.67 |
| 500 | 1.02 | 2.50 | 8.50 | 0.31 | 42.40 |

## 2. WebAssembly `opt-level = "s"` vs `opt-level = 3` (#117)

| Metric | `opt-level = "s"` | `opt-level = 3` | Delta |
| :--- | ---: | ---: | :--- |
| `.wasm` binary size | 1,500,673 B | 1,500,840 B | +167 B (+0.01%) |
| 50 tables — `analyze` / `scene` / `route` | 22.67 / 8.93 / 2.96 ms | 19.91 / 8.57 / 2.67 ms | ~1.1× faster |
| 200 tables — `analyze` / `scene` / `route` | 27.80 / 16.40 / 12.61 ms | 15.36 / 9.94 / 7.52 ms | ~1.7× faster |
| 500 tables — `analyze` / `scene` / `route` | 34.91 / 69.41 / 58.85 ms | 27.48 / 28.07 / 21.96 ms | **~2.5–2.7× faster** |

**Decision:** Switch `[profile.release.package.dbml-wasm]` in `Cargo.toml` to `opt-level = 3`. The size cost is negligible (+167 bytes) while large-diagram scene construction and edge routing are 2.5–2.7× faster.

## 3. WebAssembly Linear Memory Stability (#129)

Measured with `node scripts/bench-wasm.mjs` using `dbml_wasm::session::Session` across 200 simulated drag frames (`Session::route_edges`) on a 200-table schema:

- Initial linear memory: `1.06 MiB`
- After 50 / 200 / 500-table analysis: `1.31 MiB` / `2.00 MiB` / `3.13 MiB`
- Before 200 drag frames: `3.13 MiB`
- After 200 drag frames: **`3.13 MiB` (0.00 MiB growth)**

Because `Session` retains the parsed schema and only re-routes edges without re-copying or re-parsing DBML source on each frame, drag-time linear memory remains completely flat.
