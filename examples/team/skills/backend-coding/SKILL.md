---
id: backend-coding
name: 後端開發規範
description: NestJS 的撰寫慣例。
referenceMode: per-target
---

# 後端開發規範

- 每個 module 一個資料夾，service 不直接碰 HTTP。
- DB 存取一律走 repository 層。

@include commit-format
