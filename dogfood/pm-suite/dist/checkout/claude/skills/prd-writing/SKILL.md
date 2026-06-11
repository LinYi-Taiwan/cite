---
name: prd-writing
description: 撰寫單一產品線 PRD 的結構與驗收標準規範；指標口徑一律引用 metrics-definition，跨產品變更必須先走 cross-product-impact 流程。
---

# PRD 撰寫規範

每份 PRD 依序包含：問題 → 目標 → 範圍（in/out）→ 方案 → 驗收標準 → 指標 → Rollout。
產品線專屬的上下文（核心物件、與另一條產品線的關聯）見本 skill 的 reference。

## 驗收標準（Acceptance Criteria）寫法

每條 AC 必須是 Given / When / Then 可測句：

- **Given** 前置狀態 **When** 使用者動作 **Then** 可觀察結果
- 一條 AC 只驗一件事；複合句拆開
- 邊界條件（0、上限、逾期、斷線）至少各一條
- 不可測的形容詞（「流暢」「快速」）必須改寫成可量測門檻

## Rollout 計畫段（PRD 必備）

- 切流策略：feature flag 名稱 + 灰度梯度（內部 → 1% → 10% → 50% → 100%）
- 每階段守門指標與回滾條件（觸發即回滾，不討論）
- 跨產品 rollout 順序：被依賴方先全量，依賴方才開灰度
- 回滾劇本：誰按、按哪裡、資料是否需要清理

## 指標

PRD 內所有指標名稱與口徑必須對齊 [metrics-definition](../metrics-definition/SKILL.md) 的定義，不得在 PRD 內自創口徑。

## 跨產品變更

若本 PRD 的變更會影響另一條產品線（事件、API、UI 嵌入），先完成 [cross-product-impact](../cross-product-impact/SKILL.md) 的依賴矩陣與簽核，再進入評審。
