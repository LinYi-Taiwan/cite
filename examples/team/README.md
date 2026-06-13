# Demo：團隊開發規範 skills

一個貼近真實的 `skillc` 範例。展示三件事：

1. **`@include` 內容重用** — 共用片段寫一次，被多個 skill 引用；改一次、全部同步。
2. **per-target reference 分流** — 同一個 skill 帶多個 target 的備忘，編譯時只留當前 target 的。
3. **一份素材 → 多個 (target, agent) 成品**。

## 資料夾結構（編譯前的素材）

```text
examples/team/
├── skillc.config.yaml          # 註冊兩個 target：web、api；agent：claude
├── blocks/
│   └── commit-format.md        # 全隊共用的 commit 規範（被前後端 skill 共用）
└── skills/
    ├── frontend-coding/
    │   ├── SKILL.md            # 內含 `@include commit-format`
    │   └── references/
    │       ├── web.md          # 編 web 時保留
    │       └── api.md          # 編 web 時被丟掉（非當前 target）
    └── backend-coding/
        ├── SKILL.md            # 同樣 `@include commit-format`
        └── references/
            └── api.md
```

- `web` target 掛 `frontend-coding`，`api` target 掛 `backend-coding`（見 `skillc.config.yaml`）。
- `commit-format.md` 是共用 block：兩個 skill 都用 `@include commit-format` 引用它。

## 怎麼跑

從 repo 根目錄（`cite/`）執行已編好的 binary：

```bash
# 編 web 專案
target/release/cite build --target web --agent claude \
  --catalog examples/team --out examples/team/dist

# 編 api 專案
target/release/cite build --target api --agent claude \
  --catalog examples/team --out examples/team/dist
```

成品路徑規則是 `<--out>/<target>/<agent>/`，所以會產生：

```text
examples/team/dist/
├── web/claude/skills/frontend-coding/{SKILL.md, references/web.md}
└── api/claude/skills/backend-coding/{SKILL.md, references/api.md}
```

## 預期看到什麼

```bash
# 1) @include 被換成實際內容：原始檔只有一行 @include，成品裡是完整的 commit 規範
cat examples/team/dist/web/claude/skills/frontend-coding/SKILL.md

# 2) reference 自動分流：web 只留 web.md、api 只留 api.md
ls examples/team/dist/web/claude/skills/frontend-coding/references   # web.md
ls examples/team/dist/api/claude/skills/backend-coding/references    # api.md

# 3) 改一次、全部同步：只改共用片段，不碰任何 skill
echo '- 新規則範例。' >> examples/team/blocks/commit-format.md
target/release/cite build --target web --agent claude --catalog examples/team --out examples/team/dist
target/release/cite build --target api --agent claude --catalog examples/team --out examples/team/dist
grep -c '新規則範例' examples/team/dist/web/claude/skills/frontend-coding/SKILL.md   # → 1
grep -c '新規則範例' examples/team/dist/api/claude/skills/backend-coding/SKILL.md    # → 1
```

## manifest.json

每個 `<target>/<agent>/` 底下還有一個 `manifest.json`，記錄這份 bundle 裡有哪些 skill、各自的
`reason`（mounted / pulled-by）、inline 了哪些 block、保留了哪些 reference，以及所有 warning。
不用安裝就能 diff／檢查。

## 安裝（選用）

把編好的成品放進某個 agent 目錄（可重複執行，idempotent）：

```bash
target/release/cite install \
  --artifact examples/team/dist/web/claude --dest ~/somewhere/.claude
```
