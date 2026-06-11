---
name: backend-coding
description: NestJS 的撰寫慣例。
---

# 後端開發規範

- 每個 module 一個資料夾，service 不直接碰 HTTP。
- DB 存取一律走 repository 層。

## Commit 規範（全隊共用）

- 格式：`<type>(<scope>): <subject>`，type 用 feat / fix / chore。
- subject 用祈使句、不超過 50 字。
- 一個 commit 只做一件事。

- PR 標題沿用 commit subject。
