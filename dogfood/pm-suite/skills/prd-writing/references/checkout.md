# checkout（結帳）產品線 PRD 上下文

- 核心物件：購物車、金額計算管線、付款、訂單完成事件
- 與 loyalty 的關聯：金額計算管線內含點數折抵步驟（規則由 loyalty 提供）；「訂單完成」事件被 loyalty 消費累點
- 寫 checkout PRD 時必查：金額計算順序變更是否破壞 loyalty 折抵；事件 schema 變更是否已通知 loyalty
