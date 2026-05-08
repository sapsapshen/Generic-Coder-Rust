# 快速上手（Rust 版）

这份仓库现在以 **Rust 实现** 为主，不再要求通过 Python 入口启动。

## 1. 安装 Rust

### Windows

安装 [Rustup](https://rustup.rs/)，然后确认：

```powershell
rustc --version
cargo --version
```

### macOS / Linux

```bash
curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
rustc --version
cargo --version
```

## 2. 获取项目

```bash
git clone https://github.com/sapsapshen/Generic-Coder-Rust.git
cd Generic-Coder-Rust
```

## 3. 配置模型

有两种方式：

### 方式 A：直接在 Web UI 里配置（推荐）

启动后打开 **Settings**，填写：

- Session type
- Model name
- Base URL
- API Key

保存后即会写入本机配置。

### 方式 B：手动创建 `mykey.json`（仅限本地使用）

复制：

```text
mykey.json.example -> mykey.json
```

然后填写你的模型配置。

`mykey.json` 中包含密钥，**不要提交到 Git**。更安全的默认方式仍然是直接在 Web UI 中保存到用户本地配置。

## 4. 启动项目

### Windows 一键启动

双击根目录：

```text
start-generic-coder.bat
```

> ⏱️ **首次启动提示**：`cargo build --release` 会编译全部依赖，耗时 **2–5 分钟** 属于正常现象，之后每次启动无需重新编译。

### 手动启动

```bash
cargo run -- serve --host 127.0.0.1 --port 8765
```

浏览器打开：

```text
http://127.0.0.1:8765
```

## 5. 命令行模式

如果你想直接用 CLI：

```bash
cargo run
```

## 6. 常用能力

- Web 聊天工作台
- 多模型切换
- 工作区树 / 搜索
- Git diff / revert
- 远程 SSH
- 图片上传进上下文

### 斜杠命令（输入框中输入 `/` 即可看到提示）

| 命令 | 说明 |
|------|------|
| `/new` | 开启新会话（清除上下文） |
| `/fork` | 把当前会话 fork 为新分支 |
| `/continue <n>` | 恢复第 n 个历史会话 |
| `/plan` | 切换为计划模式（只分析，不修改文件） |
| `/work` | 切换为执行模式（实现代码） |
| `/review` | 切换为审查模式（检查问题） |
| `/clear` | 清除错误记忆和回避提示 |

## 7. 开发验证

```bash
cargo test
```

## 8. 故障排查

### 端口被占用

```
Error: Address already in use (os error 10048)
```

> 修改 `--port 8765` 参数，或关闭占用该端口的程序。

### 找不到 Chrome / 浏览器无法启动

> `Computer Use` 功能依赖本机安装 Chrome。其他功能不受影响，可正常使用。

### cargo build 失败

1. 确认 Rust 工具链已安装：`rustc --version`
2. 若版本过旧，运行 `rustup update`
3. 确认网络可连接 crates.io，或配置镜像源

### 配置文件在哪？

- **主配置**：`~/.genericagent/ui_llm_config.json`（通过 UI 保存时自动生成）
- **兼容旧配置**：项目根目录 `mykey.json`（手动编辑）
- **会话记忆**：`memory/` 目录（自动管理，无需手动修改）

