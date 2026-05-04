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

## 7. 开发验证

```bash
cargo test
```
