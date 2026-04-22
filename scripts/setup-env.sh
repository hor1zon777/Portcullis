#!/usr/bin/env bash
# 从 .env.example 生成 .env，自动填入随机密钥
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f .env ]; then
  echo "⚠ .env 已存在，跳过生成。如需重新生成请先删除 .env"
  exit 0
fi

gen_secret() {
  openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64
}

SECRET=$(gen_secret)
ADMIN_TOKEN=$(gen_secret)

cp .env.example .env

sed -i "s|CAPTCHA_SECRET=.*|CAPTCHA_SECRET=${SECRET}|" .env
sed -i "s|CAPTCHA_ADMIN_TOKEN=.*|CAPTCHA_ADMIN_TOKEN=${ADMIN_TOKEN}|" .env

echo "✔ .env 已生成"
echo "  CAPTCHA_SECRET=${SECRET:0:16}..."
echo "  CAPTCHA_ADMIN_TOKEN=${ADMIN_TOKEN:0:16}..."
echo ""
echo "下一步："
echo "  1. 编辑 .env 中的 CAPTCHA_SITES（首次 seed 站点）"
echo "  2. docker compose up -d"
