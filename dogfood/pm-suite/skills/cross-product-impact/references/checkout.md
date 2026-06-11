# checkout 視角的跨產品依賴清單

- **提供**：「訂單完成」事件給 loyalty（schema 凍結；變更走影響評估）
- **消費**：loyalty 的折抵規則（金額管線步驟之一；loyalty 失效時降級為不折抵）
