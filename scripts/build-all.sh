#!/usr/bin/env bash
# 一键构建所有产物：WASM → SDK → Rust 单二进制
set -euo pipefail
cd "$(dirname "$0")/.."

echo ">>> [1/3] 构建 WASM..."
wasm-pack build crates/captcha-wasm --target web --out-dir ../../sdk/pkg --release
echo "    WASM: $(ls -lh sdk/pkg/captcha_wasm_bg.wasm | awk '{print $5}')"

echo ">>> [2/3] 构建 SDK..."
cd sdk
pnpm install --frozen-lockfile 2>/dev/null || pnpm install
pnpm build
cd ..
echo "    SDK:  $(ls -lh sdk/dist/pow-captcha.js | awk '{print $5}')"

echo ">>> [3/3] 构建 Rust 服务（release）..."
cargo build --release -p captcha-server
echo "    BIN:  $(ls -lh target/release/captcha-server.exe 2>/dev/null || ls -lh target/release/captcha-server | awk '{print $5}')"

echo ""
echo "===== 构建完成 ====="
echo "产物："
echo "  二进制: target/release/captcha-server(.exe)"
echo ""
echo "快速启动："
echo "  ./target/release/captcha-server gen-config > captcha.toml"
echo "  # 编辑 captcha.toml 设置 secret 和 sites"
echo "  ./target/release/captcha-server --config captcha.toml"
