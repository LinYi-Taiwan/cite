# 為什麼你需要 skillc —— 給還在用資料夾管 skill 的你

## 先承認一件事

你的 skill / rules / AGENTS.md 不是文件,是**程式碼** —— agent 會逐字執行它。
但你管理它的方式,是 1995 年管理 JavaScript 的方式:散檔、複製貼上、
「改的時候記得另一邊也要改」。

資料夾管理沒有錯。**一個人、一個 agent、十幾個 skill 以內,資料夾就夠了,
你不需要這個工具。** 問題是下面四個臨界點,你遲早撞到一個,而撞到的那天,
資料夾就不夠了。

---

## 臨界點 1:團隊出現第二種 agent 的那天

你用 Claude Code,新同事用 Cursor。從這天起,每一條團隊規範存在兩份:
`.claude/skills/` 一份、`.cursor/rules/` 一份。

你改了 commit 規範。你會記得改兩邊嗎?三個月後的你會嗎?新同事會嗎?
**沒有任何東西檢查這件事** —— 兩份規範安靜地分岔,直到某天兩個人的 agent
對同一件事給出不同答案,你們花一個下午才發現原因。

skillc 的答案:規範只有一份 source,`build --all-agents` 編譯給所有 agent。
兩邊不可能不同步,因為「兩邊」不存在 —— 只有一個 source 和它的編譯產物。

## 臨界點 2:第二個專案的那天

新專案開起來,你把舊專案好用的 skill 整包抄過去。從複製那一秒起,
兩份開始各自演化。半年後它們有 60% 像、40% 不像,沒人說得清哪邊是對的。

更糟的是你**感覺不到** drift —— 沒有 diff 會跑出來告訴你「這兩份本來是同一份」。

skillc 的答案:共用的段落是 `@include` 的 block、共用的 skill 是 `imports`
的依賴,專案差異收在 per-target reference。「抄過去」這個動作被「掛進另一個
target 的 entry」取代 —— 永遠只有一份,差異是宣告出來的,不是抄出來歷史的。

## 臨界點 3:第一次想刪東西的那天

加 skill 零成本,所以 skill 只會變多。但刪一個試試:

- 它還有人用嗎?→ 不知道,只能在群組問「這個有人用嗎?」收兩個 👍 和一個「先留著吧」
- 別的 skill 有沒有引用它?→ 全文搜尋,搜的還是自然語言,「參考 code review 那份」這種句子搜得到嗎?
- 刪了之後,已經裝在 20 台機器上的那份呢?→ 它會永遠留在那裡,**繼續被 agent 觸發**

這就是為什麼每個團隊的 skill 目錄都長成只進不出的倉庫。

skillc 的答案,三條指令:

```text
$ skillc why old-deploy-rules        # 誰 include、誰 import、哪些 target 出貨 —— 編譯器的答案,不是 grep
$ skillc build                       # 沒人用的 skill 自動列出:warning[skill/orphan]
$ skillc install ...                 # 已移除的 skill 從每台機器自動退役:removed stale skill
```

「這個還有人用嗎」從社會工程問題變成指令輸出。刪除第一次變成安全的操作。

## 臨界點 4:第一次 debug「agent 為什麼這樣做」的那天

agent 做了奇怪的事。要 debug,你得知道**它當時被餵了什麼** —— 但每個人
機器上的 skill 集合都不一樣:裝的時間不同、來源不同、有人手動改過。
你連重現問題都做不到。

skillc 的答案:同樣的 catalog 永遠編出 byte-identical 的產物(CI 直接
`diff -r` 驗證),`manifest.json` 記錄每個 skill 為什麼在 bundle 裡。
「你的 agent 載入了什麼」變成一個可以回答、可以重現、可以 git blame 的問題。

---

## 「我寫個 script 就好了吧?」

我們也這麼想過。那個 script 的真實演化史:60 行 concat script → 需要分 agent
格式 → 5 支 adapter shell → 需要排除某些專案 → 305 行 bash → 沒有測試、
沒人敢動、刪 skill 還是要手動清 symlink。

你的 script 也會走完這條路,因為這些需求(多 agent、多專案、驗證、退役)
不是想像出來的,是用著用著就長出來的。差別只在:skillc 把這條路走完了,
而且帶著 11 個 test suite、確定性輸出保證和供應鏈防護(lockfile hash 釘住、
同名 id 不同來源直接紅燈)。

## 「叫大家遵守規範就好了吧?」

「改規範記得同步兩邊」「刪 skill 記得清大家機器」「引用前記得確認對方存在」——
每一條都是紀律。紀律在 3 個人時可行,10 個人時偶爾破,30 個人時必然破。
工程的歷史就是不斷把紀律換成工具:format 紀律換成 formatter,依賴紀律換成
lockfile,「記得跑測試」換成 CI。skill 管理正站在同一個換軌點上。

---

## 所以,一句話

**如果你的 skill 滿足「agent ≥ 2 或 repo ≥ 2 或 人 ≥ 3」任何一條,
你已經在付沒有 build system 的稅 —— 只是它記在「那天下午又白查了一個 bug」
的帳上,而不是工具的帳上。**

體驗成本:一個 binary,三個檔案,五分鐘。

```bash
# catalog:skills/ + blocks/ + skillc.config.yaml
skillc check                                    # 全 catalog 驗證,不寫盤
skillc build --all-targets --all-agents         # 編譯
skillc install --artifact dist/web/claude --dest ~/.claude
```

不適合就退回資料夾,你沒有損失 —— 產物就是標準的 `skills/<id>/SKILL.md`,
五家 agent(Claude / Codex / Cursor / Gemini / Copilot)原生可讀,沒有 lock-in。
