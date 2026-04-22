#!/usr/bin/env bash
# 生成 32 字节随机密钥（十六进制）
openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64
