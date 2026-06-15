# Plan：把 inspector 改成「per-repo 控制台」

## 1. 動機 / 核心轉變

使用者跑 `cite inspect serve --project .` 的意圖是 **「我只在意當前這個 repo」**。

現況問題：
- UI 呈現的是「整台機器所有安裝的 skill」(user + plugin + project) 的大清單。
- 操作大多是**全域**的：plugin 只給 "Disable (global)"(搬 quarantine，影響所有 repo)，還有 Remove(刪檔)。
- 結果：使用者最怕的「在這個 repo 關一下，結果弄壞其他 repo」反而是預設行為。

目標：**所有開關都 per-repo** —— 一律寫進「當前 repo 的 `./.claude/settings.local.json`」，
只影響這個 repo，永不碰其他 repo、永不動 skill 檔案本身、不再 quarantine、不再刪除。

## 2. 機制：每一種 skill 都能 per-repo 關（已查證官方文件）

| skill 來源 | per-repo 關閉機制 | 寫到哪 | 效果 | 來源 |
|---|---|---|---|---|
| `claude:user` | `skillOverrides[<name>]="off"` | `./.claude/settings.local.json` | 不觸發、`/skills` 也隱藏 | docs/en/skills L552-574 |
| `claude:project` | 同上 | 同上 | 同上 | 同上 |
| `claude:plugin:*` | permission `deny: ["Skill(<name>)"]` | 同上(`permissions.deny`) | 阻止自動觸發/呼叫(可能仍列在 `/skills`) | docs/en/skills L535-542 |

- 全部 **reversible**：移除對應的 key 或 deny rule 即還原，byte 無損。
- 全部只動「當前 repo」的 `settings.local.json`，其餘 key 一律保留。
- **移除** quarantine / global-disable / 刪檔 的使用者入口(那些先天是跨 repo / 破壞性的，違背本意)。

> 待實測(實作時用真 plugin 確認，不憑文件假設)：
> 1. plugin 在 deny rule 裡的確切 name —— 官方說 plugin 用 `plugin-name:skill-name` namespace (L110)，
>    所以 deny 可能要寫 `Skill(<plugin>:<skill>)` 而非裸 `Skill(<skill>)`。
> 2. permission deny 對 plugin 的實際攔截效果(是否只擋自動觸發、`/skills` 是否仍顯示)。
> 3. `settings.local.json` 的 `permissions.deny` 結構，對照 docs/en/settings 與 docs/en/permissions。

## 3. UI 改版

- 標題改成 repo 視角：`<repo 名> — skills in this project`，副標顯示 `--project` 指到的路徑。
- 每張卡只留：**名稱(可點開 markdown)+ 路徑 + 一個 on/off toggle**。(已做掉 tag / eligible / active)
- toggle off → 依來源呼叫對應機制 disable(per-repo)；toggle on → 還原。
- 移除 "Disable (global)" / "Remove…" / "Labels…" 按鈕。
- plugin 卡片加一個小註記：「plugin · 以 permission deny 關閉(在這個 repo 不觸發)」，讓使用者知道機制與「完全隱藏」略有差異。

### 分區與否(待你定)
既然**所有 skill 都能 per-repo 關**，其實不一定要分兩欄。兩個選擇：
- (A) **單一清單** + plugin 標 badge —— 最簡單，推薦。
- (B) **兩個 section**：「完全隱藏(user/project)」vs「擋觸發(plugin)」—— 視覺上把兩種效果分開。

## 4. 後端改動

- `action/` 的 `disable` 改成 per-repo 策略 dispatch：
  - `claude:user` / `claude:project` → 既有 `action/overrides.rs`(skillOverrides)✓ 已能用，不需改。
  - `claude:plugin:*` → **新增 `action/permission.rs`**：在當前 repo 的 `settings.local.json`
    的 `permissions.deny` 陣列加/移除 `Skill(<name>)`，保留其他 permission rule。
- `enable` 對應反向：移除 skillOverrides key 或移除 deny rule。
- `scan` 的 state 推導(`scan/mod.rs`)：除了讀 `skillOverrides`，再讀 `permissions.deny` 裡的
  `Skill(...)` rule，判定 plugin 在這個 repo 是否被擋 → 反映在 toggle 狀態。
- `quarantine.rs` / global / remove：後端程式碼可保留(或標 deprecated)，但**不再接到主 UI 流程**。
- `server.rs`：`/api/disable`/`/api/enable` 沿用；body 帶 `folder`(已限定為 server 的 `project_root`)。

## 5. 不變的安全原則(沿用現有)

- `scan` 純讀，永不寫；只有 toggle(`action/`)會寫，且只寫**當前 repo** 的 `settings.local.json`。
- 寫入一律保留檔案中其他 key(permissions / 其他設定)。
- markdown 閱讀面板:SKILL.md 內容先 HTML-escape 再 render(已修連結 XSS)。

## 6. 影響範圍(檔案)

- 改：`assets/inspector.html`(卡片只留 toggle、移除 global/remove、plugin badge、repo 視角標題)
- 改：`src/action/mod.rs`(disable/enable dispatch 加 plugin 分支)
- 新增：`src/action/permission.rs`(permission deny 讀寫)
- 改：`src/scan/mod.rs`(state 推導加 permissions.deny 解析)
- 改：`src/settings.rs`(加 `permissions.deny` 的讀/加/移除 helper，保留其他 key)
- 測試：`tests/action_roundtrip.rs` 加 plugin per-repo disable→enable round-trip；scan 反映 deny rule

## 7. 待你拍板的決策

1. UI 用「單一清單 + badge」(A) 還是「兩個 section」(B)？
2. plugin 的 per-repo 關閉先用 permission deny —— 接受「可能仍顯示在 `/skills`、只是不觸發」這個差異嗎？
3. global quarantine / remove 要「完全從 UI 拿掉」還是「收進一個進階/危險區」？
