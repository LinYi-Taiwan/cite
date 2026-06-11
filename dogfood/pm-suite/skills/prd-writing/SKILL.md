---
id: prd-writing
name: PRD 撰寫規範
description: 撰寫單一產品線 PRD 的結構與驗收標準規範；指標口徑一律引用 metrics-definition，跨產品變更必須先走 cross-product-impact 流程。
referenceMode: per-target
imports:
  Metrics: ../metrics-definition
  Impact: ../cross-product-impact
---

# PRD 撰寫規範

每份 PRD 依序包含：問題 → 目標 → 範圍（in/out）→ 方案 → 驗收標準 → 指標 → Rollout。
產品線專屬的上下文（核心物件、與另一條產品線的關聯）見本 skill 的 reference。

@include acceptance-criteria

@include rollout-plan

## 指標

PRD 內所有指標名稱與口徑必須對齊 {{Metrics}} 的定義，不得在 PRD 內自創口徑。

## 跨產品變更

若本 PRD 的變更會影響另一條產品線（事件、API、UI 嵌入），先完成 {{Impact}} 的依賴矩陣與簽核，再進入評審。
