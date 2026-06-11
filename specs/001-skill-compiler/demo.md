# skillc demo —— 全部是真實輸出

場景:前端團隊,兩個專案(`storefront`、`admin`),agent 用 Claude Code + Cursor。
以下每段輸出都是實際執行結果,可照打重現。

## 0. Input:整個 catalog 就 6 個檔案

```text
skillc.config.yaml
blocks/commit-format.md                      # 共用的 commit 規範
skills/react-coding/SKILL.md                 # 兩個專案都要
skills/react-coding/references/storefront.md # 只給 storefront 的補充
skills/code-review/SKILL.md                  # 被 react-coding 引用
skills/storefront-perf/SKILL.md              # 只有 storefront 要
```

```yaml
# skillc.config.yaml
targets:
  storefront:
    entry: { mounts: [react-coding, storefront-perf] }
  admin:
    entry: { mounts: [react-coding] }
agents: [claude, cursor]
```

```markdown
# skills/react-coding/SKILL.md
---
id: react-coding
name: React Coding
description: 團隊 React 寫法與 review 規範
imports:
  CodeReview: ../code-review      # ← 變數:指向另一個 skill
---

# React 規範

- function component + hooks,不寫 class
- 樣式用 Tailwind

@include commit-format             # ← 共用 block,內容會 inline 進來

寫完元件後,用 {{CodeReview}} 自我審查。
```

## 1. build:一個指令,2 專案 × 2 agent

```text
$ skillc build --all-targets --all-agents
skillc: built dist/admin/claude
skillc: built dist/admin/cursor
skillc: built dist/storefront/claude
skillc: built dist/storefront/cursor
```

編譯後的 `dist/storefront/claude/skills/react-coding/SKILL.md`:

```markdown
---
name: react-coding
description: 團隊 React 寫法與 review 規範
---

# React 規範

- function component + hooks,不寫 class
- 樣式用 Tailwind

## Commit 規範                                      ← block 內容 inline 進來了
- 格式:`<type>(<scope>): <subject>`,type 限 feat / fix / chore
- subject 祈使句,≤50 字

寫完元件後,用 [code-review](../code-review/SKILL.md) 自我審查。
                ↑ {{CodeReview}} 變成可點的連結;code-review 沒被 mount 也自動進 bundle
```

注意 admin 的 bundle:**沒有** `storefront-perf`、**沒有** `references/storefront.md`
—— admin 的 agent 不會吃到 storefront 的東西。

## 2. 改一次規範,所有地方同步

PM 要求 commit 規範加一條。改 `blocks/commit-format.md` 一個檔,rebuild:

```text
$ echo '- 一個 commit 只做一件事' >> blocks/commit-format.md
$ skillc build --all-targets --all-agents
$ diff -ru dist-before dist | grep '^+[^+]'
+- 一個 commit 只做一件事     ← admin/claude
+- 一個 commit 只做一件事     ← admin/cursor
+- 一個 commit 只做一件事     ← storefront/claude
+- 一個 commit 只做一件事     ← storefront/cursor
```

改 1 個檔,4 個 bundle 同步。Claude 和 Cursor 不可能不同步 —— 同一份 source。

## 3. 改之前先問:這會炸到誰?

```text
$ skillc why commit-format
block `commit-format`
  directly included by skills: react-coding
  directly included by blocks: (none)
  transitively inlined into skills: react-coding
  ships in targets:
    admin: react-coding (mounted)
    storefront: react-coding (mounted)
```

不用 grep、不用問人 —— 兩個專案都會收到,要不要先知會 admin 的人,你自己判斷。

## 4. 打錯字?編譯器當場抓

```text
$ skillc build --target storefront --agent claude
error[schema/invalid]: ./skills/storefront-perf/SKILL.md: malformed frontmatter:
  unknown field `descriptoin`, expected one of `id`, `name`, `description`,
  `referenceMode`, `appliesTo`, `imports` at line 3 column 1
(exit 1)
```

```text
$ skillc build --target storefront --agent claude
error[include/missing]: skill `react-coding` @includes block `comit-format` which does not exist
warning[block/unused]: block `commit-format` is included by no skill
(exit 1)                      ↑ 連「你是不是改名了」的線索都給你
```

放進 CI(`skillc check`,不寫盤),這種 PR 根本進不來。

## 5. 裝、更新、下架 —— 都是一個指令

```text
$ skillc install --artifact dist/storefront/claude --dest ~/.claude
$ ls ~/.claude/skills/
code-review   react-coding   storefront-perf
```

專案結束,`storefront-perf` 從 config 的 mounts 移除,rebuild,大家重跑 install:

```text
$ skillc install --artifact dist/storefront/claude --dest ~/.claude
skillc: removed stale skill `storefront-perf` (no longer in storefront/claude)
$ ls ~/.claude/skills/
code-review   react-coding
```

**下架的 skill 自動從每台機器退役**,不會躺在誰的目錄裡繼續被 agent 觸發。
你自己手寫的私人 skill 不會被動到(receipt 只清它自己裝過的)。

## 6. 可重現:同樣 input 永遠同樣 output

```text
$ skillc build --all-targets --all-agents --out a
$ skillc build --all-targets --all-agents --out b
$ diff -r a b && echo BYTE-IDENTICAL
BYTE-IDENTICAL
```

所以 dist/ 可以進版控:PR diff 直接顯示「每個專案的 agent 實際會收到什麼字」,
CI 用 `git diff --exit-code dist/` 擋住 source 和產物不一致。

---

## 對照表:沒有它 vs 有它

| 操作 | 資料夾管理 | skillc |
|---|---|---|
| 改共用規範 | 記得改 N 個地方 | 改 1 個檔,`build` |
| Claude+Cursor 同步 | 手動維護兩份格式 | 同一份 source 編譯 |
| 「這個誰在用?」 | 群組問 + grep | `skillc why <id>`(§3) |
| 打錯字 / 斷引用 | 沒人發現 | exit 1,指名檔案+規則(§4) |
| 下架 skill | 留在每台機器繼續觸發 | install 自動退役(§5) |
| 「agent 被餵了什麼?」 | 每台機器都不一樣 | dist 進版控,byte 級可重現(§6) |
