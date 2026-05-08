# 你以为它是一个 Coding Copilot，结果它悄悄帮你把"睡眠"都管好了

> 一个 Rust 本地 AI 编码工作台的最新进展：MCP 兼容、轻量语义层、TDD 闭环、LSP-lite、工具暴露，以及最重要的——"轻梦境"

---

有人说，程序员有三大错觉：
1. 这 bug 五分钟就能修好。
2. 明天一定开始写测试。
3. 等退休了，我就把一辈子的项目经验写成书。

前两个我们暂且不谈。但第三个——为什么非要等到退休？为什么不能在每天关电脑前，把今天干了什么自动记下来，明天打开工作台的时候，AI 助手自然就知道"昨天我们干到哪了"？

这就是 Generic Coder (Rust) 最新版里"轻梦境"（Dream Memory）的设计初衷。不过在此之前，我们先聊几个硬核更新。

---

## 一、MCP 兼容：让你的 AI 助手打通任督二脉

**MCP（Model Context Protocol）** 是 Anthropic 提出的一种标准协议，让 AI 能调用外部工具。打个比方：如果 LLM 是大脑，MCP 就是神经系统——没有它，再强的大脑也只能"脑补"。

Generic Coder 在 `src/mcp.rs` 中实现了完整的 MCP 客户端——不是调用第三方库，而是从零用 Rust 写的：

```rust
// src/mcp.rs - MCP 协议核心常量
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
```

它做了几件很硬的事：

**子进程直连。** 不走 HTTP/TCP 代理，直接 `spawn` MCP server 子进程，通过 stdin/stdout 进行 JSON-RPC 通信。这意味着零网络开销、零额外端口占用。

**会话复用。** 全局 `MCP_SESSIONS` 映射缓存已连接的 MCP server 进程，避免每次调用都重新启动。Drop 守卫自动清理僵尸进程，不留下孤儿进程。

**配置发现。** 从 `mcp_servers.json` 加载配置，支持工作区级和全局级两层合并——团队共享一批 MCP server，个人再叠加自己的，互不干扰。

结果就是：启动 Generic Coder 后，你配置的任何 MCP server（文件系统、数据库、第三方 API……）都会自动注册为 Agent 的可调用工具。**Agent 的工具箱是动态扩容的，不需要修改一行 Rust 代码。**

```
Agent 工具列表（部分）:
  mcp_list_servers  → 列出所有 MCP server
  mcp_list_tools    → 列出某个 server 提供的工具
  mcp_call_tool     → 调用具体工具
```

---

## 二、轻量语义层：不装 LSP 也能找定义

你可能会问：没有 Language Server，怎么"找到定义"、"找到引用"、"重命名预览"？

Generic Coder 的回答是：**正则驱动的零依赖语义索引**。

`src/semantic.rs` 约 660 行纯 Rust，实现了：

| 能力 | 原理 |
|------|------|
| `semantic_search` | 构建全工作区符号索引 → 加权打分 → 排序返回 |
| `lsp_find_definition` | 正则匹配 `fn/struct/enum/class/def` 声明 → 精确/前缀/包含三级匹配 |
| `lsp_find_references` | 对目标符号做 `\b` 单词边界正则全文件扫描 |
| `lsp_rename_preview` | 组合定义查找 + 引用查找 → 展示 before/after diff |
| `lsp_get_diagnostics` | 调用 `cargo check --message-format=json` → 解析编译器诊断 |

没有 LSP 服务器的安装成本，没有复杂的项目配置。支持 Rust / Python / JavaScript / TypeScript / Go / Java / Kotlin / C / C++ / C# 共 10 种语言，一个字面意义上的"lsp-lite"。

更关键的是——**它完全离线运行**。你的代码不会离开你的机器。

---

## 三、测试驱动闭环：Agent 替你跑 cargo test

"先写代码再补测试"是人类的天性。但在 Generic Coder 最新版里，Agent 拥有了一个完整的测试反馈环：

```
Agent 改代码 → run_tests → 解析失败用例 → 自动定位 → 再修改
```

核心在 `src/tools.rs` 的 `run_tests` 函数：

```rust
fn detect_test_command(root: &Path) -> Option<String> {
    if root.join("Cargo.toml").exists() {
        return Some("cargo test --quiet".to_string());
    }
    if root.join("package.json").exists() {
        return Some("npm test -- --runInBand".to_string());
    }
    if root.join("pytest.ini").exists() { ... }
}
```

它自动推断测试命令（Rust → `cargo test`，Node → `npm test`，Python → `pytest`），运行后**解析测试输出**，提取失败用例名和第一行错误信息，以结构化 JSON 返回给 Agent：

```json
{
  "feedback": {
    "kind": "cargo_test",
    "summary": "2 test(s) failed for `cargo test --quiet`",
    "failed_tests": ["test_consolidate", "test_prune"],
    "first_error": "panicked at src/dream.rs:342",
    "exit_code": 101
  }
}
```

**Agent 不再"盲写代码"——它有可验证的测试反馈。**

---

## 四、工具（技能）暴露：Agent 的能力即配置

Generic Coder 的另一个设计哲学是：**Agent 会什么，完全由配置文件说了算**。

所有工具 Schema 定义在 `assets/tools_schema.json` 中（OpenAI function-calling 格式），目前在册 **35 个工具**：

```
代码：code_run, run_tests
文件：file_read, file_write, file_patch, file_revert, file_search
搜索：content_search, semantic_search, workspace_search
LSP：lsp_find_definition, lsp_find_references, lsp_get_diagnostics, lsp_rename_preview
MCP：mcp_list_servers, mcp_list_tools, mcp_call_tool
浏览器：web_scan, web_execute_js, web_search, web_fetch, computer_screenshot, computer_open, computer_action
远程：remote_connect, remote_exec, remote_file_read, remote_file_write, remote_list_dir
Git：git_status, git_diff, git_log
媒体：media_info, media_extract
```

技能（Skills）系统更进一步——`src/skills.rs` 支持从 GitHub 远程安装技能包，7 个预设技能（CLI Anything、Brainstorming、Code Review 等）开箱即用。每个技能的状态会被注入到 Agent 的系统提示词中：

```markdown
## Active Agent Skills (7 installed, 7 enabled)

| Skill | Description | When to Use |
|-------|-------------|-------------|
| **Cli Anything** | ... | Natural language to shell commands |
| **Code Review** | ... | Reviewing code changes |
| **Self Audit** | ... | Task stalled, failures, need to pivot |
...
```

增删工具只需编辑 JSON 文件，不用动一行 Rust 逻辑。

---

## 五、轻梦境：让 AI 助手拥有"昨晚的记忆"

好了，终于要聊本文最重要的特性了。

### 问题的本质

想象一个场景：

> 你今天下午写了一个 API 鉴权模块，改了 4 个文件，调了一个小时 debug。下班时你关掉了 Generic Coder。
> 第二天早上，你打开工作台，输入"继续昨天的鉴权工作"——
> 这时候 Agent **应该知道什么**？

传统方案：
- **方案 A：** 把所有历史对话塞进上下文 → Token 爆炸，推理变慢，还烧钱。
- **方案 B：** 不保留任何历史 → Agent 完全失忆，你需要手动描述昨天做了什么。
- **方案 C：** 做一个摘要模型来总结 → 额外 LLM 调用，额外 Token 开销。

**轻梦境（Dream Memory）选择了方案 D：零额外成本。**

### 工作原理

`src/dream.rs` 的设计文档第一段就写得很清楚：

> ```
> //! Dream Memory Consolidation
> //!
> //! After each meaningful session ends, automatically extracts key facts
> //! (files changed, commands run, errors hit, user intent) and writes a
> //! lightweight JSON snapshot to memory/dreams/dream_<ts>.json.
> //!
> //! Design constraints:
> //! - Pure rule-based extraction, zero LLM calls, zero extra token cost.
> //! - Only records sessions where actual code files were changed (≥1 file_write
> //!   or file_patch), or sessions lasting ≥3 turns — skips trivial Q&A chat.
> //! - Max 20 dream files per project (oldest pruned automatically).
> //! - Injection budget: at most 5 recent entries × ~250 chars ≈ ~1,250 chars.
> ```

翻译成人话就是：

**每次会话结束时，自动从工具调用日志中提取关键信息，写成一个不到 300 字节的 JSON 文件。下次启动时，最近的 5 条"梦境记忆"会被织入 Agent 的系统提示词。**

整个过程：

1. **纯规则驱动。** 不调 LLM 做摘要，不用 embedding 做检索。就是简单的字符串提取和截断。
2. **有意义才记。** 纯闲聊会话（没有文件写入 + 少于 3 轮）自动跳过，不做无效记忆。
3. **固定预算。** 最多注入 5 条 × 约 250 字符 ≈ 1,250 字符，对 Token 消耗几乎零影响。
4. **自动清理。** 每个项目最多保留 20 条梦境记录，旧的自动删除。

### 一条"梦"长什么样

```json
{
  "timestamp": "2026-05-07T15:30:00Z",
  "intent": "为 User 模型添加邮箱验证逻辑",
  "files_changed": ["src/auth.rs", "src/models/user.rs", "tests/auth_test.rs"],
  "commands_run": ["cargo test --quiet", "cargo clippy"],
  "turns": 12,
  "errors_encountered": 2,
  "outcome": "邮箱验证逻辑已实现并通过所有测试。需要注意：正则表达式需要支持国际化域名。"
}
```

第二天，这条梦会被注入系统提示词：

```markdown
## Recent Session Memory

**[2026-05-07]** 为 User 模型添加邮箱验证逻辑
  Changed: src/auth.rs, src/models/user.rs, tests/auth_test.rs
  Ran: cargo test --quiet | cargo clippy
  Outcome: 邮箱验证逻辑已实现并通过所有测试。需要注意：正则表达式需要支持国际化域名。
```

Agent 打开新会话后，天然就知道：
- 昨天改了哪些文件 → **知道上手的上下文**
- 昨天跑过什么命令 → **知道验证流程**
- 昨天遇到了什么坑 → **避免重蹈覆辙**
- 昨天的结论是什么 → **知道下一步方向**

### 轻梦境 vs. 长眠

| | 轻梦境 | 长眠（退休后写书） |
|---|---|---|
| 发生频率 | 每次会话结束 | 一辈子一次 |
| Token 成本 | ≈0 | 不计 |
| 信息密度 | 提取关键事件 | 依赖记忆 |
| 时效性 | 最近 5 次 | 全部 |
| 实用性 | 第二天续上工作 | 留给后人 |

**轻梦境的哲学就是：每天睡一觉，醒来还记得昨天的重点。而不是等退休后再来一次长眠——那时什么都晚了。**

### 还有两个"硬记忆"加持

轻梦境不是孤立的。在系统提示词构建管线中（`src/main.rs:134-161`），还有两层记忆并行注入：

1. **技能记忆** — `skills_mgr.active_skills_summary()` → Agent 知道有哪些技能可用
2. **错误记忆** — `error_memory.avoidance_summary()` → Agent 知道哪些坑反复出现过，系统提示词中直接写"避免 X、Y、Z"

三层记忆叠加，形成了一个不靠额外 LLM 调用、不占推理 Token 的跨会话记忆体系。

---

## 六、UI/UX 六项改进：细节里的魔鬼

除了后端架构的进化，UI 端也完成了一轮"扫雷式"改进。每一个都直击用户痛点：

### 1. YOLO 模式红色警告横幅 🔴

**问题：** 用户开启 YOLO 模式（自动执行所有工具调用）后，界面上没有任何明显提示。可能在不知情的情况下让 Agent 随意修改文件。

**修复：** 在工具栏和编辑器之间增加了一个红色横幅，带脉冲动画效果，明确显示 `[YOLO MODE ACTIVE]` 和一个醒目的 "Turn off" 按钮。**它跟 `state.yoloEnabled` 响应式绑定**——开启立刻出现，关闭立刻消失。你不会再"无意识地裸奔"了。

### 2. 斜杠命令自动补全 🔍

**问题：** 用户不知道有哪些命令可用。`/new`、`/fork`、`/continue`、`/plan`、`/work`、`/review`、`/clear`——这些命令在手，但很少有人能记住全部。

**修复：** 输入 `/` 后，自动弹出下拉菜单，列出所有命令及其描述。支持 **↑/↓/Enter/Tab/Esc** 键盘导航，也支持鼠标点击插入。新用户不再需要翻文档学命令，功能自己"跳出来"了。

### 3. API Key 显隐切换 👁

**问题：** 输入 API Key 时只能盲打，不确定有没有打错字符。想看看已保存的 Key 还得去翻配置文件。

**修复：** 在 API Key 输入框旁边加了一个眼睛图标按钮。点击在 `type="password"` ↔ `type="text"` 之间切换。**这是任何现代登录表单的标配，之前竟然没有——现在有了。**

### 4. 设置字段 Tooltip 💬

**问题：** 像 Base URL 这样的字段，新用户完全不知道应该填什么格式。比如 DeepSeek 的 Base URL 和 OpenAI 的 Base URL 完全不同。

**修复：** 在 Base URL 等关键字段上添加了 `title` 属性的 tooltip，鼠标悬停即显示说明。架构上支持扩展到更多字段，保持简洁但不失指引。

### 5. 首次构建友好提示 ⏱️

**问题：** 用户双击 `start-generic-coder.bat`，然后看到 `cargo build --release` 跑了好几分钟还没反应，以为程序卡死了。**在 Rust 项目里，"编译慢"是第一个劝退点。**

**修复：** 在批处理脚本中增加了明确的提示：

```
⏱️ 首次启动提示：cargo build --release 会编译全部依赖，
耗时 2–5 分钟属于正常现象，之后每次启动无需重新编译。
```

设定了预期，用户就不会在 3 分钟时按 Ctrl+C 了。

同时 `GETTING_STARTED.md` 也增加了完整的故障排查章节：端口冲突怎么办、Chrome 找不到怎么办、构建失败怎么排查、配置文件在哪——把"我该去哪找"的焦虑降到最低。

### 6. 主题 & 视觉打磨

整体 UI 采用 **Apple 字体系统**（`-apple-system, 'SF Pro Text', 'SF Pro Display'`），窗口默认 1440×960，所有按钮和图标放大至 2.5 倍。工作流节点支持拖拽连线（Work → Plan → Review 管道可视化），工作区侧栏可折叠，输入栏支持 ArrowUp/ArrowDown 草稿恢复，Token 用量在左下角实时统计。

**从"能看"到"看得舒服"，是一个开源工具走向成熟的关键一步。**

---

## 写在最后

如果你问我，这个项目最独特的价值观是什么：

**不是"做一个最强的 AI"，而是"做一个每天都陪在你身边的 AI"。**

轻梦境的存在，恰恰说明这个项目不是一个炫技的技术 Demo——它在认真思考一个实际问题：**怎么让明天的 AI 助手，记得昨天的你干了什么**。

MCP 兼容让它能接入不断增长的工具生态。轻量语义层让代码导航不再依赖重型 LSP。TDD 闭环让 Agent 有自我验证能力。而轻梦境，让这一切——

**在每天"醒来"的那一刻，无缝衔接。**

---

> 项目地址：[https://github.com/sapsapshen/Generic-Coder-Rust](https://github.com/sapsapshen/Generic-Coder-Rust)
>
> 一键启动：`start-generic-coder.bat`（Windows）或 `bash start-generic-coder.sh`（macOS/Linux）
>
> 开源协议：MIT

---

*本文基于 Generic Coder (Rust) main 分支截至 2026-05-08 的最新代码撰写。所有代码引用均可追溯到源文件对应行号。*
