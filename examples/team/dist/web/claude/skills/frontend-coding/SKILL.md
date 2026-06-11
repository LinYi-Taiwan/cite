---
name: frontend-coding
description: React/TS 的撰寫與 review 慣例。
---

# 前端開發規範

- 元件一律 function component + hooks。
- 樣式用 Tailwind，不寫 inline style。

## Commit 規範（全隊共用）

- 格式：`<type>(<scope>): <subject>`，type 用 feat / fix / chore。
- subject 用祈使句、不超過 50 字。
- 一個 commit 只做一件事。

- PR 標題沿用 commit subject。
