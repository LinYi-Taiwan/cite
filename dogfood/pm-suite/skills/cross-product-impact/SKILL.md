---
id: cross-product-impact
name: 跨產品影響評估
description: 一條產品線的變更會影響另一條產品線時的依賴盤點、簽核與 rollout 順序流程；loyalty 與 checkout 的互相依賴用此流程管理。
referenceMode: per-target
imports:
  Metrics: ../metrics-definition
---

# 跨產品影響評估

任何跨產品介面（事件 / API / UI 嵌入）的變更，依下列流程處理。
本產品線視角的依賴清單見本 skill 的 reference。

@include dependency-matrix

## 簽核流程

1. 提供方 PM 填妥依賴矩陣，列出全部消費方
2. 每個消費方 PM 在 PRD 上簽核，並補上降級行為
3. 守門指標沿用 {{Metrics}} 的口徑，提供方與消費方各掛一組

## Rollout 順序

被依賴方（提供方）先全量，消費方才開灰度——順序寫進雙方 PRD 的 rollout 段。
