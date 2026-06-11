---
name: cross-product-impact
description: 一條產品線的變更會影響另一條產品線時的依賴盤點、簽核與 rollout 順序流程；loyalty 與 checkout 的互相依賴用此流程管理。
---

# 跨產品影響評估

任何跨產品介面（事件 / API / UI 嵌入）的變更，依下列流程處理。
本產品線視角的依賴清單見本 skill 的 reference。

## 跨產品依賴矩陣

| 提供方 | 消費方 | 介面 | 變更通知方式 | 失效影響 |
|---|---|---|---|---|
| (產品A) | (產品B) | (event / API / UI 嵌入) | (PRD 互審 / changelog) | (降級行為) |

規則：

- 任何跨產品介面變更，提供方 PRD 必須列出所有消費方並取得簽核
- 消費方必須定義提供方失效時的降級行為（fail-open / fail-closed）

## 簽核流程

1. 提供方 PM 填妥依賴矩陣，列出全部消費方
2. 每個消費方 PM 在 PRD 上簽核，並補上降級行為
3. 守門指標沿用 [metrics-definition](../metrics-definition/SKILL.md) 的口徑，提供方與消費方各掛一組

## Rollout 順序

被依賴方（提供方）先全量，消費方才開灰度——順序寫進雙方 PRD 的 rollout 段。
