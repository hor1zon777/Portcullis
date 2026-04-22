# ========== Stage 1: WASM 构建 ==========
FROM rust:1-slim AS wasm-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN wasm-pack build crates/captcha-wasm \
    --target web --release --out-dir /wasm-out

# ========== Stage 2: SDK 构建 ==========
FROM node:22-alpine AS sdk-builder

RUN npm install -g pnpm@10
WORKDIR /sdk

COPY sdk/package.json sdk/pnpm-lock.yaml* sdk/tsconfig.json sdk/vite.config.ts ./
RUN pnpm install --frozen-lockfile || pnpm install

COPY sdk/src ./src
COPY sdk/index.html ./
COPY --from=wasm-builder /wasm-out ./pkg
RUN pnpm build

# ========== Stage 3: Rust 服务构建（嵌入资源）==========
FROM rust:1-slim AS rust-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

COPY --from=sdk-builder /sdk/dist ./sdk/dist
COPY --from=wasm-builder /wasm-out ./sdk/pkg

RUN cargo build --release -p captcha-server

# ========== Stage 4: 运行时 ==========
FROM gcr.io/distroless/cc-debian12

COPY --from=rust-builder /build/target/release/captcha-server /captcha-server

EXPOSE 8787
ENTRYPOINT ["/captcha-server"]
