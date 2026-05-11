# 我给自己写了个「AI 编码副驾」，Rust 写的，还塞了个四冲程发动机

> 这不是又一个 AI 套壳玩具，而是一个从底层一步步搓出来的本地编码工作台。

掐指一算，我折腾 **Generic Coder** 这个 Rust 原生 AI 编码工作台已经好一阵子了。之前的主要矛盾是：面对不同任务，同一个 Agent 行为策略总得调——查个问题用深度推理太慢，写代码又嫌轻量模式不够给力。

于是我干了一件事：**给它装了个四冲程发动机**。

## 🔍 Ask · 📋 Plan · 🔨 Build · 🔎 Review

这四个模式不是简单改改 prompt 就完事。每个模式都是独立的行为策略，且都支持 **100 轮超长任务预算**：

| 模式 | 一句话 |
|------|--------|
| **Ask** 🔍 | 轻量推理，直接作答，适合问问题和查资料 |
| **Plan** 📋 | 先出结构化计划再执行，适合复杂任务拆解 |
| **Build** 🔨 | 全工具调用循环，适合写代码、改代码、跑命令 |
| **Review** 🔎 | 读代码库，输出结构化审计报告 |

更骚的是**自动模式识别**：不选模式时，Agent 会自动分析输入内容的关键词，预选最合适的模式。当然，你可以随时手动覆盖，**用户选择永远优先**。

而说到交互优化，这次的细节打磨我自己都很满意：

- **Enter 发送，Shift+Enter 换行**——听起来简单？我花了不少功夫把中文拼音输入法的确认回车正确排除掉，IME 感知可不是一句空话
- **ArrowUp/ArrowDown 翻历史命令**——每一条历史都带着附件状态一起恢复，不掉链子
- **红色 ■ 停止按钮**——一键终止所有后台 Agent 任务
- **Agent Logs 开关**——想看推理过程就开着，嫌吵就关掉，Streaming 输出零额外延迟

## 🚄 再说说那个被低估的 LLM 优化器管道

大多数「本地编码工具」只是包了一层 API 调用。我给 Generic Coder 加了实打实的四阶段优化管道（`temp/optimizer/` crate），目前已上线两个阶段：

**P0 – 网络层**：HTTP/2 连接池消除 TCP 握手开销；SSE 零拷贝解析器让流式 token 处理的 CPU 占用降了 50%；前缀缓存感知的 Prompt 编排能最大限度提高 DeepSeek 的缓存命中率。

**P1 – 内存层**：`bumpalo` Arena 分配器把请求级内存分配开销砍了 80%；基于信息熵的动态 Prompt 压缩会在发送前剔除低信息量 token；请求优先级队列确保用户可见的请求优先于后台预取。

P2（自适应限流 + 多模型智能路由）正在施工中，P3（SIMD 加速分词 + 本地 KV 缓存降级）已经排上日程。

## 🖥 视觉上的那些事

UI 方面也做了不少「看不见但舒服」的改进：

- 字体系统全面切换为 **Apple 原生字体栈**（`-apple-system` + SF Pro），所有按钮图标**放大 2.5 倍**到 40×40px，窗口默认 1440×960
- **Workflow Builder 节点拖拽**——Work → Plan → Review 管道可视化，拖拽连线
- **Token 用量实时统计**——左下角模型状态卡按命令聚合，不玩虚的
- **工作区文件树可折叠**，平滑动画，不占视觉空间
- **文件行内预览**——点击文件直接在会话区渲染文本或图片
- **Chrome 路径自动检测**——Computer Use 自动找 Chrome/Chromium，macOS/Linux/Windows 全平台支持

放两张图感受一下：

**聊天工作区**
<img width="1440" alt="Chat Workspace" src="https://github.com/user-attachments/assets/855d51e3-8cd5-4462-9f73-6e3ac95aaea1" />

**工作流拖拽 Builder**
<img width="1440" alt="Workflow Builder" src="https://github.com/user-attachments/assets/94e4da03-fa3c-4fa2-844b-dc638c32395a" />

## 🛠 最后说点实在的

这段时间的优化，目标一直没变：**让 Generic Coder 成为一个真正能日常使用的本地 AI 编码工作台**，而不是一个跑个 demo 就吃灰的玩具。

- **内置 15+ LLM 提供商预设**（DeepSeek / Qwen / Kimi / MiniMax / Doubao / OpenAI / Anthropic / OpenRouter……），换模型不需要翻文档查 Base URL
- **ACP 多智能体协作**（Orchestrator + Specialist 架构），复杂任务真的可以拆给多个 Agent 并行
- **One Shot 自主模式**，外层发散 + 内层执行，适合需要脑暴的任务
- **持久化错误记忆**——同样的错误不犯第二次
- **SSH 远程连接**，远程开发和本地开发用同一个界面
- **7 项预设技能**，还能远程安装新技能

哦对，现在还有 **Electron macOS 安装包**（arm64 + x64 都有），装了就完事，不用配环境。

项目完全开源（MIT），地址在 [github.com/sapsapshen/Generic-Coder-Rust](https://github.com/sapsapshen/Generic-Coder-Rust)，启动也很简单：

```bash
cargo run -- serve --host 127.0.0.1 --port 8765
```

或者 macOS/Linux 用户直接 `bash start-generic-coder.sh`，Windows 双击 `start-generic-coder.bat`，打开浏览器就开干。

---

*这就是一个工程师的「晚上和周末项目」，一路写到今天的样子。欢迎 Star，欢迎 PR，更欢迎提 issue 告诉我哪里还能更骚。*
