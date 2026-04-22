#!/usr/bin/env bash
# 本地开发：同时启动 Rust 验证服务和 Vite SDK 开发服务器
set -euo pipefail

cd "$(dirname "$0")/.."

# 检查 CAPTCHA_SECRET
export CAPTCHA_SECRET="${CAPTCHA_SECRET:-this-is-a-local-dev-secret-key-32+}"
export CAPTCHA_BIND="${CAPTCHA_BIND:-127.0.0.1:8787}"
export CAPTCHA_SITES='{"pk_test":{"secret_key":"sk_test_secret","diff":18,"origins":["http://localhost:5173"]}}'

echo ">>> 启动验证服务 ($CAPTCHA_BIND)..."
cargo run -p captcha-server &
SERVER_PID=$!

echo ">>> 启动 Vite SDK 开发服务器 (:5173)..."
cd sdk && pnpm dev &
VITE_PID=$!

trap "kill $SERVER_PID $VITE_PID 2>/dev/null; exit" INT TERM

echo ""
echo "===================="
echo " 打开 http://localhost:5173"
echo " Ctrl+C 停止所有服务"
echo "===================="
echo ""

wait
