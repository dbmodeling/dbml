# syntax=docker/dockerfile:1

# ==============================================================================
# Stage 0: Node.js runtime source (matches CI / release.yml)
# ==============================================================================
FROM node:24-bookworm-slim AS node-base

# ==============================================================================
# Stage 1: Build Wasm (dbml-wasm) + Web workbench (packages/canvas-web)
# ==============================================================================
FROM rust:1.99.1-bookworm AS web-builder

WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=node-base /usr/local/bin/node /usr/local/bin/node
COPY --from=node-base /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -sf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -sf /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx

RUN rustup target add wasm32-unknown-unknown

# Install the exact wasm-bindgen-cli version locked in Cargo.lock (cached by Cargo.lock)
COPY Cargo.lock ./
RUN LOCKED_BINDGEN="$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[version = "]/, ""); print; exit}' Cargo.lock)" \
    && cargo install --locked wasm-bindgen-cli --version "${LOCKED_BINDGEN}"

# Install frontend dependencies (cached by package-lock.json)
COPY packages/canvas-web/package.json packages/canvas-web/package-lock.json packages/canvas-web/
RUN cd packages/canvas-web && npm ci

# Copy workspace sources and build crates/cli/web/
COPY Cargo.toml ./
COPY assets ./assets
COPY crates ./crates
COPY packages/canvas-web ./packages/canvas-web
COPY scripts/build-web-assets.sh ./scripts/build-web-assets.sh

RUN mkdir -p crates/cli/web && ./scripts/build-web-assets.sh

# ==============================================================================
# Stage 2: Build the native Rust CLI binary (dbml) with embedded web assets
# ==============================================================================
FROM rust:1.99.1-bookworm AS cli-builder

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web-builder /src/crates/cli/web ./crates/cli/web

RUN cargo build --release --locked -p dbml-cli

# ==============================================================================
# Stage 3: Minimal non-root runtime image
# ==============================================================================
FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.title="DB Modeling Web Server" \
      org.opencontainers.image.description="Interactive DBML visual database modeling web server (dbml CLI)" \
      org.opencontainers.image.source="https://github.com/dbmodeling/dbml" \
      org.opencontainers.image.licenses="PolyForm-Noncommercial-1.0.0"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 dbml \
    && useradd --uid 1000 --gid dbml --create-home --shell /usr/sbin/nologin dbml \
    && mkdir -p /workspace \
    && chown dbml:dbml /workspace

COPY --from=cli-builder /src/target/release/dbml /usr/local/bin/dbml

ENV PORT=3000 \
    HOST=0.0.0.0

WORKDIR /workspace
VOLUME ["/workspace"]
EXPOSE 3000

USER dbml

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT}/health" || exit 1

ENTRYPOINT ["dbml"]
CMD ["/workspace", "--host", "0.0.0.0", "--port", "3000", "--no-open"]
