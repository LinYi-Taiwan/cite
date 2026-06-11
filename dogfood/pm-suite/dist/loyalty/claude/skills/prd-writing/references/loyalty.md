# loyalty（會員點數）產品線 PRD 上下文

- 核心物件：點數帳戶、累點規則、折抵規則、效期
- 與 checkout 的關聯：折抵 UI 嵌入在 checkout 流程內；checkout 的「訂單完成」事件是累點的唯一來源
- 寫 loyalty PRD 時必查：折抵上限規則是否影響 checkout 的金額計算順序
