# 前端開發情境:從零導入到日常使用

團隊:前端 6 人,兩個 repo(`storefront`、`admin-dashboard`),agent 用 Claude Code,兩人用 Cursor。

---

## 1. 建 catalog(一次性)

開一個 repo `fe-skills`:

```text
fe-skills/
├── skillc.config.yaml
├── blocks/
│   ├── commit-format.md          # commit/PR 規範
│   ├── component-conventions.md  # 元件寫法:function component、props 命名、檔案結構
│   └── e2e-selectors.md          # data-testid 命名規則
├── skills/
│   ├── react-coding/SKILL.md     # 兩個 repo 都要
│   ├── storefront-perf/SKILL.md  # 只有 storefront 要(LCP/CLS 預算、圖片規範)
│   └── admin-tables/SKILL.md     # 只有 admin 要(表格/表單模式)
└── dist/                         # 編譯產物,進版控
```

```yaml
# skillc.config.yaml
targets:
  storefront:
    entry: { mounts: [react-coding, storefront-perf] }
  admin:
    entry: { mounts: [react-coding, admin-tables] }
agents: [claude, cursor]
```

```markdown
# skills/react-coding/SKILL.md
---
id: react-coding
name: React Coding
description: 團隊 React 寫法與 review 規範
---

@include component-conventions
@include commit-format
@include e2e-selectors
```

```bash
skillc build --all-targets --all-agents   # 產出 dist/{storefront,admin}/{claude,cursor}/
```

## 2. 每個人裝(一次性)

```bash
# storefront 開發者
skillc install --artifact dist/storefront/claude --dest ~/.claude
# 用 Cursor 的人
skillc install --artifact dist/storefront/cursor --dest ~/repos/storefront/.cursor
```

之後更新 = `git pull && skillc install ...`(可包成 alias 或 post-merge hook)。

## 3. 日常:改一條規範

PM 要求 commit message 加上 ticket 號。改 `blocks/commit-format.md` 一個檔:

```bash
skillc build --all-targets --all-agents
git add -A && git commit && git push   # 開 PR
```

PR diff 同時顯示:block 改了什麼 + 4 個 bundle 裡哪些 SKILL.md 跟著變。
reviewer 看得到「admin 跟 storefront 都會收到」。merge 後大家 pull + install。

## 4. 日常:加一個只給 storefront 的 skill

新需求:storefront 要接 A/B testing SDK,寫 `skills/ab-testing/SKILL.md`,
然後掛進 target:

```yaml
  storefront:
    entry: { mounts: [react-coding, storefront-perf, ab-testing] }
```

忘了掛?build 會提醒:

```text
warning[skill/orphan]: skill `ab-testing` is reached by no entry or import
```

admin 的人 install 後**不會**拿到這個 skill —— 不在它的 bundle 裡。

## 4b. 日常:skill 引用 skill(變數語義)

`react-coding` 想說「寫完元件後,用 code-review skill 自我審查」。不寫散文,
宣告成 import:

```markdown
---
id: react-coding
imports:
  CodeReview: ../code-review     # 像 JS 的 import,Alias 是變數
---

寫完元件後,接下來使用 {{CodeReview}} 做自我審查。
```

編譯後:

```markdown
寫完元件後,接下來使用 [code-review](../code-review/SKILL.md) 做自我審查。
```

編譯器保證(跟 JS 一樣的語義):

- `code-review` **就算沒被 mount,也自動進 bundle**(manifest 記 `pulled-by:react-coding`)
- **遞迴**:被引用的 skill 自己 import 的 skill、自己的 `references/`,任何深度都跟著進來,
  它體內的 `{{}}` 同樣被解析成連結(`main → jira → bitbucket` 三層有回歸測試)
- 引用未宣告的變數 → `error[marker/undefined]`,編譯不過
- 多個 skill 引用同一個依賴 → 去重,只包一份;循環引用安全(視為已共同打包)
- 跨 repo 引用(`imports: { Jira: other-repo:jira }`)→ lockfile 釘 contentHash

跟 `@include` 的分工:**`@include` 是內容展開**(規範文字直接 inline,像 macro),
**`{{Alias}}` 是引用**(指向另一個完整 skill,保證它在場,像 import)。

## 5. 日常:改共用 block 前看影響範圍

要改 `e2e-selectors.md` 的命名規則,先看誰會被影響:

```text
$ skillc why e2e-selectors
block `e2e-selectors`
  directly included by skills: react-coding
  ships in targets:
    admin: react-coding (mounted)
    storefront: react-coding (mounted)
```

兩個 repo 都吃到 → 變更要先跟 QA 對齊,而不是 merge 後才發現。

## 6. 日常:下架 skill

A/B testing 專案結束,從 mounts 移除 `ab-testing`。下次大家 install:

```text
skillc: removed stale skill `ab-testing` (no longer in storefront/claude)
```

不會殘留在任何人的 agent 目錄裡繼續誤觸發。

## 7. CI(catalog repo,一次性設定)

```yaml
- run: skillc check --frozen                                  # 驗證,不寫盤
- run: skillc build --all-targets --all-agents --frozen
- run: git diff --exit-code dist/                             # dist 必須跟 source 一致
```

打錯 frontmatter、`@include` 不存在的 block、忘了 per-target reference
→ PR 直接紅,錯誤指名檔案和規則。

---

## 各角色實際碰到的指令

| 角色 | 會用到的 |
|---|---|
| 一般成員 | `git pull && skillc install ...`(通常包成 hook,等於零指令) |
| 改規範的人 | 編輯 md → `skillc build` → 開 PR;改共用 block 前 `skillc why <id>` |
| maintainer | 管 `skillc.config.yaml` 的 mounts;看 orphan warning 決定刪什麼 |
| CI | `check` + `build` + `git diff --exit-code dist/` |
