# 一键构建所有产物（Windows PowerShell）
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)

Write-Host ">>> [1/3] 构建 WASM..."
wasm-pack build crates/captcha-wasm --target web --out-dir ../../sdk/pkg --release

Write-Host ">>> [2/3] 构建 SDK..."
Set-Location sdk
pnpm install
pnpm build
Set-Location ..

Write-Host ">>> [3/3] 构建 Rust 服务（release）..."
cargo build --release -p captcha-server

Write-Host ""
Write-Host "===== 构建完成 ====="
Write-Host "产物: target\release\captcha-server.exe"
Write-Host ""
Write-Host "快速启动:"
Write-Host "  .\target\release\captcha-server.exe gen-config > captcha.toml"
Write-Host "  # 编辑 captcha.toml"
Write-Host "  .\target\release\captcha-server.exe --config captcha.toml"
