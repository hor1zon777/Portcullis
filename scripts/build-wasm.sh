#!/usr/bin/env bash
# 构建 WASM：从 crates/captcha-wasm 编译并输出到 sdk/pkg/
set -euo pipefail

cd "$(dirname "$0")/.."
echo ">>> 构建 captcha-wasm (release)..."
wasm-pack build crates/captcha-wasm --target web --out-dir ../../sdk/pkg --release
echo ">>> WASM 输出: sdk/pkg/"
ls -lh sdk/pkg/*.wasm
