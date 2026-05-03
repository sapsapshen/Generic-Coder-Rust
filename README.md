# Generic Coder (Rust)

Rust-native coding cockpit with a built-in web UI, local workspace tools, Git review flows, SSH support, and configurable LLM backends.

**Language / Idioma / 语言:** [中文](#zh) | [English](#en) | [Español](#es)

---

<a id="zh"></a>

## 中文

### 这是什么

Generic Coder：https://github.com/sapsapshen/Generic-Coder
现已切换为 **Rust 主实现**。项目通过本地 Web UI 提供一个统一工作台，用于：

- 与模型对话并执行编码任务
- 切换和保存多套模型配置
- 打开本地工作区、查看文件树、搜索文件
- 查看 Git 变更、差异与回退信息
- 连接远程 SSH 工作环境

当前推荐入口：

- **Windows 一键启动：** `start-generic-coder.bat`
- **命令行启动：** `cargo run -- serve --host 127.0.0.1 --port 8765`

服务默认运行在：

```text
http://127.0.0.1:8765
```

### 当前实现状态

Rust 版本已经接管核心运行路径：

- `src\main.rs`：CLI 与服务启动
- `src\web.rs`：Web UI 后端
- `src\agent.rs`：Agent 循环与任务执行
- `src\llm.rs`：Claude / OpenAI 兼容 / 推理与流式解析
- `src\tools.rs`、`src\workspace.rs`、`src\remote.rs`：工具、工作区、远程环境

旧 Python 代码已不再是默认启动路径。

### 已支持的能力

- Rust + Axum Web 服务
- 聊天工作台与任务轮询
- 多模型配置、切换与本地持久化
- 本地工作区选择：支持图形点选和手动输入路径
- Git 变更查看、差异预览、回退辅助
- 远程 SSH 连接与文件/命令操作
- 图片上传到上下文
- 主题切换与多主题 UI

### 模型配置

可以通过两种方式配置模型：

1. **推荐：** 启动后在 Web UI 的 **Settings** 中直接填写
2. 在项目根目录放置 `mykey.json`（参考 `mykey.json.example`）

UI 已内置常见预设，支持直接填写 API Key 使用，包括：

- DeepSeek
- Qwen / DashScope
- Kimi / Moonshot
- MiniMax
- Doubao / Ark
- Tencent Hunyuan
- Baidu Qianfan
- Zhipu
- OpenAI / Anthropic / OpenRouter

也支持手动填写：

- Session type
- Base URL
- Provider
- Model name
- API Key

UI 保存的配置默认写入当前用户目录，不会自动写回仓库文件。

### 快速开始

#### 1. 安装 Rust

```powershell
rustc --version
cargo --version
```

如果没有 Rust，请先安装 [Rustup](https://rustup.rs/)。

#### 2. 获取代码

```bash
git clone https://github.com/sapsapshen/Generic-Coder-Rust.git
cd Generic-Coder-Rust
```

#### 3. 启动

**Windows**

双击：

```text
start-generic-coder.bat
```

**macOS / Linux / 手动**

```bash
cargo run -- serve --host 127.0.0.1 --port 8765
```

#### 4. 打开浏览器

```text
http://127.0.0.1:8765
```

### 开发与验证

```bash
cargo test
```

### 目录结构

```text
src\
  main.rs       CLI + 服务启动
  web.rs        Web UI 后端
  agent.rs      Agent 循环
  llm.rs        模型接入与流式解析
  tools.rs      工具集合
  workspace.rs  工作区管理
  remote.rs     SSH 远程环境
  media.rs      媒体处理
  config.rs     配置加载与保存
assets\
  generic_coder\  Web 前端资源
```

---

<a id="en"></a>

## English

### What it is

Generic Coder：https://github.com/sapsapshen/Generic-Coder
is now a **Rust-first** coding cockpit. It provides a local web interface for:

- chatting with an LLM-driven coding agent
- saving and switching model configurations
- opening a local workspace, browsing the tree, and searching files
- reviewing Git changes and diffs
- connecting to a remote SSH environment

Recommended entry points:

- **Windows one-click launcher:** `start-generic-coder.bat`
- **Manual startup:** `cargo run -- serve --host 127.0.0.1 --port 8765`

Default local URL:

```text
http://127.0.0.1:8765
```

### Current implementation status

The Rust runtime now owns the supported execution path:

- `src\main.rs` - CLI and server startup
- `src\web.rs` - web backend
- `src\agent.rs` - agent loop and task execution
- `src\llm.rs` - Claude / OpenAI-compatible backends and streaming parsing
- `src\tools.rs`, `src\workspace.rs`, `src\remote.rs` - tools, workspace, and remote environment support

The legacy Python entrypoints are no longer the primary startup path.

### Included capabilities

- Rust + Axum web server
- chat workspace with task polling
- multi-model configuration and local persistence
- local workspace selection with both folder picker and direct path input
- Git change review, diff preview, and revert helpers
- remote SSH connection and file/command operations
- image upload into the chat context
- theme switching and multiple UI themes

### Model configuration

You can configure models in two ways:

1. **Recommended:** save them in **Settings** from the web UI
2. Add a `mykey.json` file in the project root based on `mykey.json.example`

The UI includes ready-to-use presets for common providers:

- DeepSeek
- Qwen / DashScope
- Kimi / Moonshot
- MiniMax
- Doubao / Ark
- Tencent Hunyuan
- Baidu Qianfan
- Zhipu
- OpenAI / Anthropic / OpenRouter

Manual configuration is also supported for:

- session type
- base URL
- provider
- model name
- API key

Saved UI configurations are written to the local user profile rather than committed to the repository.

### Quick start

#### 1. Install Rust

```bash
rustc --version
cargo --version
```

If Rust is not installed yet, use [Rustup](https://rustup.rs/).

#### 2. Clone the repository

```bash
git clone https://github.com/sapsapshen/Generic-Coder-Rust.git
cd Generic-Coder-Rust
```

#### 3. Start the app

**Windows**

Double-click:

```text
start-generic-coder.bat
```

**macOS / Linux / manual**

```bash
cargo run -- serve --host 127.0.0.1 --port 8765
```

#### 4. Open the UI

```text
http://127.0.0.1:8765
```

### Development

```bash
cargo test
```

### Project structure

```text
src\
  main.rs       CLI + server startup
  web.rs        Web UI backend
  agent.rs      Agent loop
  llm.rs        Model integration and streaming parser
  tools.rs      Tool implementations
  workspace.rs  Workspace manager
  remote.rs     SSH remote support
  media.rs      Media handling
  config.rs     Config loading and persistence
assets\
  generic_coder\  Web frontend assets
```

---

<a id="es"></a>

## Español

### Qué es

Generic Coder：https://github.com/sapsapshen/Generic-Coder
ahora funciona con una implementación **principalmente en Rust**. Ofrece una interfaz web local para:

- conversar con un agente de programación basado en LLM
- guardar y cambiar configuraciones de modelos
- abrir un espacio de trabajo local, ver el árbol y buscar archivos
- revisar cambios y diffs de Git
- conectarse a un entorno remoto por SSH

Entradas recomendadas:

- **Inicio con un clic en Windows:** `start-generic-coder.bat`
- **Inicio manual:** `cargo run -- serve --host 127.0.0.1 --port 8765`

URL local por defecto:

```text
http://127.0.0.1:8765
```

### Estado actual de la implementación

La ruta de ejecución soportada ya está controlada por Rust:

- `src\main.rs` - CLI e inicio del servidor
- `src\web.rs` - backend de la interfaz web
- `src\agent.rs` - bucle del agente y ejecución de tareas
- `src\llm.rs` - backends compatibles con Claude / OpenAI y parsing de streaming
- `src\tools.rs`, `src\workspace.rs`, `src\remote.rs` - herramientas, espacio de trabajo y entorno remoto

Las rutas antiguas en Python ya no son la vía principal de inicio.

### Capacidades incluidas

- servidor web Rust + Axum
- espacio de chat con sondeo de tareas
- configuración múltiple de modelos con persistencia local
- selección de espacio de trabajo local por selector gráfico o ruta manual
- revisión de cambios Git, vista previa de diff y ayuda para revertir
- conexión SSH remota y operaciones de archivos/comandos
- subida de imágenes al contexto del chat
- cambio de tema y varios temas de interfaz

### Configuración de modelos

Puedes configurar modelos de dos formas:

1. **Recomendado:** desde **Settings** en la interfaz web
2. Creando `mykey.json` en la raíz del proyecto a partir de `mykey.json.example`

La UI ya incluye preajustes para proveedores comunes:

- DeepSeek
- Qwen / DashScope
- Kimi / Moonshot
- MiniMax
- Doubao / Ark
- Tencent Hunyuan
- Baidu Qianfan
- Zhipu
- OpenAI / Anthropic / OpenRouter

También se puede configurar manualmente:

- tipo de sesión
- base URL
- proveedor
- nombre del modelo
- API key

Las configuraciones guardadas desde la UI se escriben en el perfil local del usuario, no en el repositorio.

### Inicio rápido

#### 1. Instala Rust

```bash
rustc --version
cargo --version
```

Si aún no tienes Rust, instala [Rustup](https://rustup.rs/).

#### 2. Clona el repositorio

```bash
git clone https://github.com/sapsapshen/Generic-Coder-Rust.git
cd Generic-Coder-Rust
```

#### 3. Inicia la aplicación

**Windows**

Haz doble clic en:

```text
start-generic-coder.bat
```

**macOS / Linux / manual**

```bash
cargo run -- serve --host 127.0.0.1 --port 8765
```

#### 4. Abre la interfaz

```text
http://127.0.0.1:8765
```

### Desarrollo

```bash
cargo test
```

### Estructura del proyecto

```text
src\
  main.rs       CLI + inicio del servidor
  web.rs        Backend de la interfaz web
  agent.rs      Bucle del agente
  llm.rs        Integración de modelos y parser de streaming
  tools.rs      Implementación de herramientas
  workspace.rs  Gestión del espacio de trabajo
  remote.rs     Soporte SSH remoto
  media.rs      Manejo de medios
  config.rs     Carga y persistencia de configuración
assets\
  generic_coder\  Recursos del frontend web
```
