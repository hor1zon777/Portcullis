# 从 .env.example 生成 .env，自动填入随机密钥（Windows PowerShell）

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)

if (Test-Path .env) {
    Write-Host "⚠ .env 已存在，跳过生成。如需重新生成请先删除 .env"
    exit 0
}

function New-Secret {
    -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
}

$secret = New-Secret
$adminToken = New-Secret

Copy-Item .env.example .env

(Get-Content .env) -replace 'CAPTCHA_SECRET=.*', "CAPTCHA_SECRET=$secret" |
    Set-Content .env
(Get-Content .env) -replace 'CAPTCHA_ADMIN_TOKEN=.*', "CAPTCHA_ADMIN_TOKEN=$adminToken" |
    Set-Content .env

Write-Host "✔ .env 已生成"
Write-Host "  CAPTCHA_SECRET=$($secret.Substring(0,16))..."
Write-Host "  CAPTCHA_ADMIN_TOKEN=$($adminToken.Substring(0,16))..."
Write-Host ""
Write-Host "下一步："
Write-Host "  1. 编辑 .env 中的 CAPTCHA_SITES（首次 seed 站点）"
Write-Host "  2. docker compose up -d"
