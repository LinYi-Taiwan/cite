# loyalty 視角的跨產品依賴清單

- **消費**：checkout 的「訂單完成」事件（累點唯一來源；schema 變更需走簽核）
- **提供**：折抵規則計算（被 checkout 金額管線呼叫；失效時 fail-open＝不折抵照常結帳）
