"use strict";
(() => {
  // workbench/src/base/common/lifecycle.ts
  function toDisposable(fn) {
    return { dispose: fn };
  }
  var DisposableStore = class {
    disposables = /* @__PURE__ */ new Set();
    isDisposed = false;
    add(disposable) {
      if (this.isDisposed) {
        disposable.dispose();
        return disposable;
      }
      this.disposables.add(disposable);
      return disposable;
    }
    clear() {
      for (const disposable of this.disposables) {
        disposable.dispose();
      }
      this.disposables.clear();
    }
    dispose() {
      if (this.isDisposed) {
        return;
      }
      this.isDisposed = true;
      this.clear();
    }
  };
  var Disposable = class {
    store = new DisposableStore();
    _register(disposable) {
      return this.store.add(disposable);
    }
    dispose() {
      this.store.dispose();
    }
  };

  // workbench/src/platform/instantiation/common/instantiationService.ts
  var InstantiationService = class {
    constructor(services2) {
      this.services = services2;
    }
    createInstance(ctor, ...args) {
      return new ctor(this, ...args);
    }
    get(id) {
      return this.services.get(id);
    }
  };

  // workbench/src/platform/instantiation/common/serviceCollection.ts
  var ServiceCollection = class {
    entries = /* @__PURE__ */ new Map();
    set(id, instance) {
      this.entries.set(id, instance);
      return instance;
    }
    get(id) {
      const value = this.entries.get(id);
      if (!value) {
        throw new Error(`Missing service: ${String(id.description || id.toString())}`);
      }
      return value;
    }
  };

  // workbench/src/workbench/common/dom.ts
  function escapeHtml(text) {
    const raw = text == null ? "" : String(text);
    return raw.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
  }
  function inferLanguage(filePath) {
    const extension = filePath.split(".").pop()?.toLowerCase();
    switch (extension) {
      case "rs":
        return "rust";
      case "ts":
      case "tsx":
        return "typescript";
      case "js":
      case "mjs":
      case "cjs":
        return "javascript";
      case "json":
        return "json";
      case "md":
        return "markdown";
      case "toml":
        return "ini";
      case "yml":
      case "yaml":
        return "yaml";
      case "html":
        return "html";
      case "css":
        return "css";
      case "sh":
        return "shell";
      case "diff":
      case "patch":
        return "diff";
      default:
        return "plaintext";
    }
  }

  // workbench/src/platform/instantiation/common/serviceIdentifier.ts
  function createServiceIdentifier(name) {
    return Symbol(name);
  }

  // workbench/src/workbench/services/serviceIds.ts
  var ICommandService = createServiceIdentifier("commandService");
  var ILayoutService = createServiceIdentifier("layoutService");
  var INotificationService = createServiceIdentifier("notificationService");
  var IWorkbenchService = createServiceIdentifier("workbenchService");

  // workbench/src/workbench/parts/activitybarPart.ts
  var ActivitybarPart = class extends Disposable {
    layoutService;
    workbenchService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    render() {
      const items = [
        { id: "chat", icon: "comment-discussion", label: "Assistant" },
        { id: "explorer", icon: "files", label: "Explorer" },
        { id: "scm", icon: "source-control", label: "Changes" },
        { id: "workflow", icon: "server-process", label: "Workflow" },
        { id: "extensions", icon: "extensions", label: "Skills" },
        { id: "settings", icon: "settings-gear", label: "Settings" }
      ];
      const container = this.layoutService.getElement("activitybar");
      container.innerHTML = items.map(
        (item) => `
          <button class="activitybar__item${item.id === this.workbenchService.state.activeView ? " is-active" : ""}" data-view="${item.id}" title="${escapeHtml(item.label)}">
            <i class="codicon codicon-${item.icon}"></i>
          </button>
        `
      ).join("");
      container.querySelectorAll("[data-view]").forEach((button) => {
        button.addEventListener("click", () => {
          this.workbenchService.setActiveView(button.dataset.view);
        });
      });
    }
  };

  // workbench/src/workbench/parts/composerPart.ts
  var ComposerPart = class _ComposerPart extends Disposable {
    mentionSuggestions = [];
    loopCheckTimer = null;
    slashSelectedIndex = 0;
    static SLASH_COMMANDS = [
      { cmd: "/new", desc: "Start a fresh session (clears context)" },
      { cmd: "/fork", desc: "Fork the current session into a new branch" },
      { cmd: "/continue", desc: "/continue <n>  \u2014 restore and resume session #n" },
      { cmd: "/plan", desc: "Switch to Plan mode (explore without modifying files)" },
      { cmd: "/work", desc: "Switch to Work mode (implement and execute)" },
      { cmd: "/review", desc: "Switch to Review mode (audit code for issues)" },
      { cmd: "/clear", desc: "Clear error memory and avoidance hints" }
    ];
    layoutService;
    workbenchService;
    commandService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this.commandService = accessor.get(ICommandService);
      this.bind();
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    bind() {
      const input = this.layoutService.getElement("prompt-input");
      const sendButton = this.layoutService.getElement("send-button");
      const modeSelect = this.layoutService.getElement("mode-select");
      const multiAgentToggle = this.layoutService.getElement("multi-agent-toggle");
      const oneShotToggle = this.layoutService.getElement("one-shot-toggle");
      const loopToggle = this.layoutService.getElement("loop-toggle");
      const workflowFollowToggle = this.layoutService.getElement("workflow-follow-toggle");
      const computerUseToggle = this.layoutService.getElement("computer-use-toggle");
      const yoloToggle = this.layoutService.getElement("yolo-toggle");
      const autoModelToggle = this.layoutService.getElement("auto-model-toggle");
      const effortGroup = this.layoutService.getElement("effort-group");
      const yoloWarningOff = this.layoutService.getElement("yolo-warning-off");
      yoloWarningOff.addEventListener("click", () => {
        void this.workbenchService.toggleYolo(false);
      });
      input.addEventListener("input", async () => {
        this.workbenchService.setInputValue(input.value);
        this.updateSlashHints(input);
        await this.refreshMentionSuggestions();
        this.scheduleLoopCheck(input.value);
      });
      input.addEventListener("keydown", async (event) => {
        const slashHints = this.layoutService.getElement("slash-hints");
        if (!slashHints.hidden) {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            this.slashSelectedIndex = Math.min(this.slashSelectedIndex + 1, this.visibleSlashCommands(input.value).length - 1);
            this.renderSlashHints(input, slashHints);
            return;
          }
          if (event.key === "ArrowUp") {
            event.preventDefault();
            this.slashSelectedIndex = Math.max(this.slashSelectedIndex - 1, 0);
            this.renderSlashHints(input, slashHints);
            return;
          }
          if (event.key === "Enter" || event.key === "Tab") {
            const cmds = this.visibleSlashCommands(input.value);
            if (cmds[this.slashSelectedIndex]) {
              event.preventDefault();
              input.value = cmds[this.slashSelectedIndex].cmd + " ";
              this.workbenchService.setInputValue(input.value);
              slashHints.hidden = true;
              return;
            }
          }
          if (event.key === "Escape") {
            event.preventDefault();
            slashHints.hidden = true;
            return;
          }
        }
        const mod = event.metaKey || event.ctrlKey;
        if (mod && event.key.toLowerCase() === "enter") {
          event.preventDefault();
          slashHints.hidden = true;
          await this.workbenchService.sendPrompt(input.value);
          return;
        }
        if (event.key === "Tab" && this.mentionSuggestions[0] && input.value.includes("@")) {
          event.preventDefault();
          const nextValue = this.workbenchService.insertMention(this.mentionSuggestions[0].rel || this.mentionSuggestions[0].path, input.selectionStart);
          input.value = nextValue;
        }
      });
      const keydownHandler = (event) => {
        const mod = event.metaKey || event.ctrlKey;
        const key = event.key.toLowerCase();
        if (mod && key === "p") {
          event.preventDefault();
          void this.commandService.executeCommand("workbench.action.quickOpen");
          return;
        }
        if (mod && event.shiftKey && key === "n") {
          event.preventDefault();
          void this.commandService.executeCommand("workbench.action.newChat");
          return;
        }
        if (event.key === "Escape") {
          void this.commandService.executeCommand("workbench.action.closeQuickOpen");
        }
      };
      document.addEventListener("keydown", keydownHandler);
      this._register(
        toDisposable(() => {
          document.removeEventListener("keydown", keydownHandler);
        })
      );
      sendButton.addEventListener("click", () => {
        void this.workbenchService.sendPrompt(input.value);
      });
      modeSelect.addEventListener("change", () => {
        void this.workbenchService.setMode(modeSelect.value);
      });
      multiAgentToggle.addEventListener("change", () => {
        void this.workbenchService.toggleMultiAgent(multiAgentToggle.checked, input.value);
      });
      oneShotToggle.addEventListener("change", () => {
        void this.workbenchService.toggleOneShot(oneShotToggle.checked);
      });
      loopToggle.addEventListener("change", () => {
        void this.workbenchService.toggleLoop(loopToggle.checked);
      });
      workflowFollowToggle.addEventListener("change", () => {
        void this.workbenchService.toggleWorkflowFollow(workflowFollowToggle.checked);
      });
      computerUseToggle.addEventListener("change", () => {
        void this.workbenchService.toggleComputerUse(computerUseToggle.checked);
      });
      yoloToggle.addEventListener("change", () => {
        void this.workbenchService.toggleYolo(yoloToggle.checked);
      });
      autoModelToggle.addEventListener("change", () => {
        void this.workbenchService.toggleAutoModel(autoModelToggle.checked);
      });
      effortGroup.querySelectorAll(".effort-btn").forEach((btn) => {
        btn.addEventListener("click", () => {
          const effort = btn.dataset.effort;
          const current = this.workbenchService.state.reasoningEffort;
          void this.workbenchService.setReasoningEffort(current === effort ? null : effort);
        });
      });
    }
    render() {
      const state = this.workbenchService.state;
      const input = this.layoutService.getElement("prompt-input");
      const modeSelect = this.layoutService.getElement("mode-select");
      const multiAgentToggle = this.layoutService.getElement("multi-agent-toggle");
      const oneShotToggle = this.layoutService.getElement("one-shot-toggle");
      const loopToggle = this.layoutService.getElement("loop-toggle");
      const workflowFollowToggle = this.layoutService.getElement("workflow-follow-toggle");
      const workflowFollowLabel = this.layoutService.getElement("workflow-follow-label");
      const computerUseToggle = this.layoutService.getElement("computer-use-toggle");
      const yoloToggle = this.layoutService.getElement("yolo-toggle");
      const autoModelToggle = this.layoutService.getElement("auto-model-toggle");
      const effortGroup = this.layoutService.getElement("effort-group");
      const sendButton = this.layoutService.getElement("send-button");
      const meta = this.layoutService.getElement("composer-meta");
      if (input.value !== state.inputValue) {
        input.value = state.inputValue;
      }
      modeSelect.value = state.currentMode;
      multiAgentToggle.checked = state.multiAgentEnabled;
      oneShotToggle.checked = state.oneShotEnabled;
      loopToggle.disabled = !state.loopAvailable;
      loopToggle.checked = state.loopEnabled;
      loopToggle.title = state.loopAvailable ? "Repeat this task in a loop" : "Loop not available for this task";
      workflowFollowToggle.checked = state.workflowFollowEnabled;
      workflowFollowToggle.disabled = state.workflowNodes.length === 0;
      workflowFollowToggle.title = state.workflowNodes.length === 0 ? "No workflow steps configured \u2014 open the Workflow panel" : "Strictly follow the workflow pipeline order";
      if (state.workflowNodes.length > 0) {
        workflowFollowLabel.textContent = `Workflow: ${state.workflowNodes.map((n) => n.mode).join(" \u2192 ")}`;
      } else {
        workflowFollowLabel.textContent = "Workflow";
      }
      computerUseToggle.checked = state.computerUseEnabled;
      computerUseToggle.disabled = !state.computerUseAvailable;
      computerUseToggle.title = state.computerUseAvailable ? "Allow AI to control mouse, keyboard and screen" : "Computer Use is not available on this platform";
      yoloToggle.checked = state.yoloEnabled;
      yoloToggle.title = state.yoloEnabled ? "YOLO: AI executes all actions autonomously without confirmation" : "Enable YOLO mode for fully autonomous execution";
      const yoloWarning = this.layoutService.getElement("yolo-warning");
      yoloWarning.hidden = !state.yoloEnabled;
      autoModelToggle.checked = state.autoModelEnabled;
      autoModelToggle.title = state.autoModelEnabled ? "DeepSeek-style automatic model + reasoning routing is active" : "Automatically route each turn to the best model";
      effortGroup.querySelectorAll(".effort-btn").forEach((btn) => {
        const effort = btn.dataset.effort;
        btn.classList.toggle("effort-btn--active", state.reasoningEffort === effort);
        btn.disabled = state.autoModelEnabled;
        btn.title = effort === "off" ? "Disable reasoning" : effort === "high" ? "High reasoning effort" : "Maximum reasoning effort";
      });
      sendButton.disabled = state.isRunning;
      meta.textContent = state.isRunning ? "Task running" : state.autoModelEnabled && state.autoRoute ? `Auto -> ${state.autoRoute.model}${state.autoRoute.reasoning_effort ? ` / ${state.autoRoute.reasoning_effort}` : ""}` : state.autoModelEnabled ? "Auto routing enabled" : "";
    }
    scheduleLoopCheck(prompt) {
      if (this.loopCheckTimer !== null) {
        clearTimeout(this.loopCheckTimer);
      }
      this.loopCheckTimer = setTimeout(() => {
        this.loopCheckTimer = null;
        void this.workbenchService.checkLoopSuitability(prompt);
      }, 200);
    }
    async refreshMentionSuggestions() {
      const input = this.layoutService.getElement("prompt-input");
      const before = input.value.slice(0, input.selectionStart);
      const atIndex = before.lastIndexOf("@");
      if (atIndex < 0) {
        this.mentionSuggestions = [];
        return;
      }
      const query = before.slice(atIndex + 1).trim();
      this.mentionSuggestions = await this.workbenchService.fetchMentionSuggestions(query);
    }
    visibleSlashCommands(text) {
      const lower = text.toLowerCase();
      return _ComposerPart.SLASH_COMMANDS.filter((c) => c.cmd.startsWith(lower) || lower === "/");
    }
    updateSlashHints(input) {
      const slashHints = this.layoutService.getElement("slash-hints");
      const text = input.value;
      if (!text.startsWith("/") || text.includes(" ") || text.includes("\n")) {
        slashHints.hidden = true;
        return;
      }
      this.slashSelectedIndex = 0;
      this.renderSlashHints(input, slashHints);
    }
    renderSlashHints(input, container) {
      const cmds = this.visibleSlashCommands(input.value);
      if (cmds.length === 0) {
        container.hidden = true;
        return;
      }
      container.hidden = false;
      container.innerHTML = cmds.map(
        (c, i) => `
          <button class="slash-hints__item${i === this.slashSelectedIndex ? " is-selected" : ""}" data-slash-cmd="${c.cmd}">
            <span class="slash-hints__cmd">${c.cmd}</span>
            <span class="slash-hints__desc">${c.desc}</span>
          </button>`
      ).join("");
      container.querySelectorAll("[data-slash-cmd]").forEach((btn, i) => {
        btn.addEventListener("mouseenter", () => {
          this.slashSelectedIndex = i;
          this.renderSlashHints(input, container);
        });
        btn.addEventListener("click", () => {
          input.value = btn.dataset.slashCmd + " ";
          this.workbenchService.setInputValue(input.value);
          container.hidden = true;
          input.focus();
        });
      });
    }
  };

  // workbench/src/generated/monacoWorkers.ts
  var monacoWorkerFiles = {
    "editor": "editor.worker-Be8ye1pW.js",
    "json": "json.worker-DKiEKt88.js",
    "css": "css.worker-HnVq6Ewq.js",
    "html": "html.worker-B51mlPHg.js",
    "ts": "ts.worker-CMbG-7ft.js"
  };

  // workbench/src/monaco.ts
  var monacoPromise = null;
  function selectWorkerFile(label) {
    if (label === "json") {
      return monacoWorkerFiles.json || monacoWorkerFiles.editor;
    }
    if (label === "css" || label === "scss" || label === "less") {
      return monacoWorkerFiles.css || monacoWorkerFiles.editor;
    }
    if (label === "html" || label === "handlebars" || label === "razor") {
      return monacoWorkerFiles.html || monacoWorkerFiles.editor;
    }
    if (label === "typescript" || label === "javascript") {
      return monacoWorkerFiles.ts || monacoWorkerFiles.editor;
    }
    return monacoWorkerFiles.editor;
  }
  function loadMonaco() {
    if (window.monaco) {
      return Promise.resolve(window.monaco);
    }
    if (monacoPromise) {
      return monacoPromise;
    }
    monacoPromise = new Promise((resolve, reject) => {
      const loader = window.require;
      if (!loader || !loader.config) {
        reject(new Error("Monaco loader is unavailable"));
        return;
      }
      window.MonacoEnvironment = {
        getWorker(_, label) {
          const file = selectWorkerFile(label);
          if (!file) {
            throw new Error(`Missing Monaco worker for ${label}`);
          }
          return new Worker(`/static/vendor/monaco/vs/assets/${file}`, {
            name: label
          });
        }
      };
      loader.config({ paths: { vs: "/static/vendor/monaco/vs" } });
      loader(["vs/editor/editor.main"], () => {
        if (!window.monaco) {
          reject(new Error("Monaco failed to initialize"));
          return;
        }
        resolve(window.monaco);
      });
    }).catch((error) => {
      monacoPromise = null;
      throw error;
    });
    return monacoPromise;
  }

  // workbench/src/workbench/parts/editorPart.ts
  function renderMessageContent(raw, streaming) {
    const sanitized = raw.replace(/<(?:tool_use|tool_call)>[\s\S]*?(?:<\/(?:tool_use|tool_call)>|$)/g, "").replace(/<summary>[\s\S]*?(?:<\/summary>|$)/g, "").replace(/<\/?(?:summary|tool_use|tool_call)>/g, "").trim();
    const thinkingRe = /<thinking>([\s\S]*?)<\/thinking>/g;
    const parts = [];
    let last = 0;
    let m;
    while ((m = thinkingRe.exec(sanitized)) !== null) {
      if (m.index > last) {
        parts.push(`<pre class="message__text">${escapeHtml(sanitized.slice(last, m.index))}</pre>`);
      }
      parts.push(
        `<details class="thinking-block" open><summary class="thinking-block__summary"><i class="codicon codicon-lightbulb"></i> Reasoning</summary><pre class="thinking-block__content">${escapeHtml(m[1].trim())}</pre></details>`
      );
      last = m.index + m[0].length;
    }
    const tail = sanitized.slice(last);
    if (tail) {
      parts.push(`<pre class="message__text${streaming ? " message__text--streaming" : ""}">${escapeHtml(tail)}</pre>`);
    }
    return parts.join("") || `<pre class="message__text${streaming ? " message__text--streaming" : ""}"></pre>`;
  }
  var EditorPart = class extends Disposable {
    editorInstance = null;
    editorModel = null;
    renderKey = "";
    layoutService;
    workbenchService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    render() {
      this.renderTabs();
      const activeTab = this.workbenchService.state.tabs.find((tab) => tab.id === this.workbenchService.state.activeTabId) || this.workbenchService.state.tabs[0];
      if (!activeTab) {
        return;
      }
      const nextKey = this.computeRenderKey(activeTab);
      if (nextKey === this.renderKey) {
        return;
      }
      this.renderKey = nextKey;
      const surface = this.layoutService.getElement("editor-surface");
      if (activeTab.kind === "chat") {
        this.disposeEditor();
        surface.innerHTML = `
        <div class="chat-feed" id="chat-feed">
          ${this.workbenchService.state.messages.map(
          (message) => `
                <article class="message message--${escapeHtml(message.role)}">
                  <div class="message__avatar"><i class="codicon codicon-${message.role === "user" ? "account" : "sparkle"}"></i></div>
                  <div class="message__body">
                    <div class="message__role">${escapeHtml(message.role)}</div>
                    <div class="message__content">${renderMessageContent(message.content || "", message.streaming)}</div>
                  </div>
                </article>`
        ).join("")}
        </div>`;
        const feed = surface.querySelector("#chat-feed");
        if (feed) {
          feed.scrollTop = feed.scrollHeight;
        }
        return;
      }
      if (activeTab.kind === "preview") {
        const preview = activeTab.preview;
        surface.innerHTML = `
        <div class="editor-header">
          <div>
            <strong>${escapeHtml(preview.rel || preview.name)}</strong>
            <div class="muted">${escapeHtml(preview.mime)} \xB7 ${preview.size} bytes${preview.truncated ? " \xB7 truncated" : ""}</div>
          </div>
          <div class="editor-header__actions">
            <button class="text-button" data-copy-path="${escapeHtml(preview.rel || preview.path)}">Mention</button>
          </div>
        </div>
        <div class="preview-host" id="preview-host"></div>`;
        surface.querySelector("[data-copy-path]")?.addEventListener("click", () => {
          this.workbenchService.insertMention(preview.rel || preview.path);
        });
        void this.renderPreview(activeTab.preview);
        return;
      }
      surface.innerHTML = `
      <div class="editor-header">
        <div><strong>${escapeHtml(activeTab.path)}</strong><div class="muted">Unified diff</div></div>
      </div>
      <div class="preview-host" id="preview-host"></div>`;
      void this.renderTextEditor(activeTab.diff, "diff");
    }
    renderTabs() {
      const tabs = this.layoutService.getElement("editor-tabs");
      tabs.innerHTML = this.workbenchService.state.tabs.map((tab) => {
        const closable = tab.id !== "chat";
        return `
          <button class="tab${tab.id === this.workbenchService.state.activeTabId ? " is-active" : ""}" data-tab-id="${tab.id}">
            <span>${escapeHtml(tab.title)}</span>
            ${closable ? `<i class="codicon codicon-close" data-close-tab="${tab.id}"></i>` : ""}
          </button>`;
      }).join("");
      tabs.querySelectorAll("[data-tab-id]").forEach((button) => {
        button.addEventListener("click", (event) => {
          const closeTarget = event.target.closest("[data-close-tab]");
          if (closeTarget) {
            const id2 = closeTarget.getAttribute("data-close-tab");
            if (id2) {
              this.workbenchService.closeTab(id2);
            }
            return;
          }
          const id = button.dataset.tabId;
          if (id) {
            this.workbenchService.setActiveTab(id);
          }
        });
      });
    }
    async renderPreview(preview) {
      const host = this.layoutService.getElement("preview-host");
      if (preview.kind === "image") {
        this.disposeEditor();
        host.innerHTML = `<div class="image-preview"><img src="/api/workspace/preview-content?path=${encodeURIComponent(preview.path)}" alt="${escapeHtml(preview.name)}" /></div>`;
        return;
      }
      if (preview.kind === "binary") {
        await this.renderTextEditor(preview.message || "Binary preview is not supported.", "plaintext");
        return;
      }
      await this.renderTextEditor(preview.content || "", inferLanguage(preview.path));
    }
    async renderTextEditor(value, language) {
      const host = this.layoutService.getElement("preview-host");
      host.innerHTML = "";
      const monaco = await loadMonaco();
      this.disposeEditor();
      this.editorModel = monaco.editor.createModel(value, language);
      this.editorInstance = monaco.editor.create(host, {
        model: this.editorModel,
        readOnly: true,
        automaticLayout: true,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontLigatures: true,
        theme: this.workbenchService.getEditorTheme()
      });
    }
    computeRenderKey(activeTab) {
      if (activeTab.kind === "chat") {
        const lastMessage = this.workbenchService.state.messages[this.workbenchService.state.messages.length - 1];
        return `chat:${this.workbenchService.state.activeTabId}:${this.workbenchService.state.messages.length}:${lastMessage?.content || ""}`;
      }
      if (activeTab.kind === "preview") {
        return `preview:${activeTab.path}:${activeTab.preview.kind}:${activeTab.preview.size}:${this.workbenchService.state.theme}`;
      }
      return `diff:${activeTab.path}:${activeTab.diff.length}:${this.workbenchService.state.theme}`;
    }
    disposeEditor() {
      this.editorInstance?.dispose();
      this.editorInstance = null;
      this.editorModel?.dispose();
      this.editorModel = null;
    }
  };

  // workbench/src/workbench/parts/quickOpenPart.ts
  var QuickOpenPart = class extends Disposable {
    currentQuery = "";
    currentVisibility = false;
    layoutService;
    workbenchService;
    commandService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this.commandService = accessor.get(ICommandService);
      this.bind();
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    bind() {
      const shell = this.layoutService.getElement("quick-open");
      const scrim = shell.querySelector(".quick-open__scrim");
      const input = this.layoutService.getElement("quick-open-input");
      const close = () => {
        this.hideShell(shell);
        this.workbenchService.setQuickOpenVisible(false);
      };
      input.addEventListener("input", () => {
        this.currentQuery = input.value;
        void this.renderResults();
      });
      input.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close();
          return;
        }
        if (event.key === "Enter") {
          const first = this.layoutService.getElement("quick-open-results").querySelector("[data-quick-kind]");
          if (first) {
            event.preventDefault();
            void this.executeItem(first.dataset.quickKind || "", first.dataset.quickValue || "");
          }
        }
      });
      scrim?.addEventListener("pointerdown", (event) => {
        event.preventDefault();
        close();
      });
      shell.addEventListener("pointerdown", (event) => {
        if (event.target === shell) {
          event.preventDefault();
          close();
        }
      });
      const keydownHandler = (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close();
        }
      };
      document.addEventListener("keydown", keydownHandler, true);
      this._register(toDisposable(() => document.removeEventListener("keydown", keydownHandler, true)));
    }
    render() {
      const visible = this.workbenchService.state.quickOpenVisible;
      const shell = this.layoutService.getElement("quick-open");
      shell.hidden = !visible;
      shell.classList.toggle("is-visible", visible);
      shell.setAttribute("aria-hidden", String(!visible));
      if (visible && !this.currentVisibility) {
        this.currentQuery = "";
        const input = this.layoutService.getElement("quick-open-input");
        input.value = "";
        void this.renderResults();
        input.focus();
      }
      this.currentVisibility = visible;
    }
    hideShell(shell) {
      shell.hidden = true;
      shell.classList.remove("is-visible");
      shell.setAttribute("aria-hidden", "true");
      this.currentVisibility = false;
    }
    async renderResults() {
      const results = this.layoutService.getElement("quick-open-results");
      const query = this.currentQuery.toLowerCase();
      const commands = this.commandService.getCommands().filter((command) => command.label.toLowerCase().includes(query)).map((command) => ({ kind: "command", label: command.label, value: command.id }));
      const files = await this.workbenchService.getQuickOpenFileResults(this.currentQuery);
      const rows = [...commands, ...files];
      results.innerHTML = rows.map(
        (row) => `
          <button class="quick-open__item" data-quick-kind="${row.kind}" data-quick-value="${escapeHtml(row.value)}">
            <i class="codicon codicon-${row.kind === "file" ? "file" : "terminal-cmd"}"></i>
            <span>${escapeHtml(row.label)}</span>
          </button>`
      ).join("");
      results.querySelectorAll("[data-quick-kind]").forEach((button) => {
        button.addEventListener("click", () => {
          void this.executeItem(button.dataset.quickKind || "", button.dataset.quickValue || "");
        });
      });
    }
    async executeItem(kind, value) {
      this.workbenchService.setQuickOpenVisible(false);
      if (kind === "command") {
        await this.commandService.executeCommand(value);
        return;
      }
      if (kind === "file") {
        await this.workbenchService.openPreviewTab(value);
      }
    }
  };

  // workbench/src/workbench/common/state.ts
  var DEFAULT_LLM_FORM = {
    entry_key: "generic_coder_native_oai_config",
    session_type: "native_oai",
    protocol_preset: "custom",
    api_mode: "chat_completions",
    provider: "",
    name: "",
    apibase: "",
    apikey: "",
    model: ""
  };
  var DEFAULT_REMOTE_FORM = {
    enabled: false,
    server_name: "",
    name: "",
    host: "",
    port: 22,
    username: "root",
    password: "",
    key_path: "",
    cwd: ""
  };
  var THEME_OPTIONS = ["graphite", "obsidian", "cobalt", "daybreak", "paperink", "solarflare"];
  function createInitialWorkbenchState() {
    return {
      activeView: "chat",
      activeTabId: "chat",
      tabs: [{ id: "chat", title: "Chat", kind: "chat" }],
      messages: [],
      sessions: [],
      workspaceTree: [],
      changes: [],
      skills: [],
      llmForm: { ...DEFAULT_LLM_FORM },
      providerProfiles: [],
      workspace: { active: null, workspaces: [], recent_folders: [] },
      remote: { form: { ...DEFAULT_REMOTE_FORM }, configs: [], active_connections: [], connected: false },
      models: [],
      currentModelIndex: 0,
      theme: "graphite",
      isRunning: false,
      pendingTaskId: null,
      taskPlaceholderIndex: null,
      modelLabel: "Model offline",
      workspacePickerToken: "",
      workspaceCollapsedPaths: [],
      workspaceDraftName: "",
      workspaceDraftPath: "",
      workflowNodes: [],
      workflowActive: false,
      workflowCurrentNode: 0,
      workflowFollowEnabled: false,
      loopEnabled: false,
      loopAvailable: false,
      computerUseEnabled: false,
      computerUseAvailable: false,
      yoloEnabled: false,
      reasoningEffort: null,
      autoModelEnabled: false,
      autoRoute: null,
      lastUsage: null,
      sessionUsage: null,
      currentSessionIndex: null,
      currentSessionActiveCheckpoint: null,
      currentSessionCheckpoints: [],
      checkpointPanelSessionIndex: null,
      checkpointPanelEntries: [],
      currentMode: "work",
      multiAgentEnabled: false,
      oneShotEnabled: false,
      planRemaining: -1,
      quickOpenVisible: false,
      inputValue: ""
    };
  }

  // workbench/src/workbench/parts/sidebarPart.ts
  var MODEL_PRICING = {
    "deepseek-v4-pro": { cacheHit: 3625e-6, cacheMiss: 0.435, output: 0.87 },
    "deepseek-v4-flash": { cacheHit: 28e-4, cacheMiss: 0.14, output: 0.28 },
    "deepseek-reasoner": { cacheHit: 3625e-6, cacheMiss: 0.435, output: 0.87 },
    "deepseek-chat": { cacheHit: 28e-4, cacheMiss: 0.14, output: 0.28 }
  };
  function estimateCost(model, promptTokens, completionTokens, cachedTokens) {
    const pricing = Object.entries(MODEL_PRICING).find(([key]) => model.toLowerCase().includes(key))?.[1];
    if (!pricing) {
      return "n/a";
    }
    const missTokens = Math.max(promptTokens - cachedTokens, 0);
    const cost = cachedTokens / 1e6 * pricing.cacheHit + missTokens / 1e6 * pricing.cacheMiss + completionTokens / 1e6 * pricing.output;
    return cost < 1e-3 ? "<$0.001" : `$${cost.toFixed(4)}`;
  }
  function hitRate(promptTokens = 0, cachedTokens = 0) {
    if (!promptTokens) {
      return "0%";
    }
    return `${Math.round(cachedTokens / promptTokens * 100)}%`;
  }
  function normalizeProviderBase(base) {
    return base.trim().toLowerCase().replace(/\/beta$/, "").replace(/\/v1$/, "");
  }
  function sanitizePreview(preview) {
    return preview.replace(/<thinking>[\s\S]*?(?:<\/thinking>|$)/gi, "").replace(/<summary>[\s\S]*?(?:<\/summary>|$)/gi, "").replace(/<(?:tool_use|tool_call)>[\s\S]*?(?:<\/(?:tool_use|tool_call)>|$)/gi, "").replace(/<\/?(?:thinking|summary|tool_use|tool_call)>/gi, "").replace(/\s+/g, " ").trim();
  }
  function matchProviderProfileId(state) {
    const currentBase = (state.llmForm.apibase || "").trim().toLowerCase();
    const currentModel = (state.llmForm.model || "").trim().toLowerCase();
    const normalizedCurrentBase = normalizeProviderBase(currentBase);
    const exactMatch = state.providerProfiles.find(
      (profile) => normalizeProviderBase(profile.apibase) === normalizedCurrentBase && profile.model.trim().toLowerCase() === currentModel
    );
    const modelMatch = state.providerProfiles.find((profile) => profile.model.trim().toLowerCase() === currentModel);
    const providerMatch = state.providerProfiles.find(
      (profile) => normalizeProviderBase(profile.apibase) === normalizedCurrentBase
    );
    return exactMatch?.id || modelMatch?.id || providerMatch?.id || state.providerProfiles[0]?.id || "";
  }
  var SidebarPart = class extends Disposable {
    layoutService;
    workbenchService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    render() {
      const state = this.workbenchService.state;
      const title = this.layoutService.getElement("sidebar-title");
      const content = this.layoutService.getElement("sidebar-content");
      const titles = {
        chat: "Assistant",
        explorer: "Explorer",
        scm: "Source Control",
        extensions: "Skills",
        settings: "Settings",
        workflow: "Workflow"
      };
      title.textContent = titles[state.activeView];
      switch (state.activeView) {
        case "chat":
          content.innerHTML = this.renderChat();
          this.bindChat(content);
          break;
        case "explorer":
          content.innerHTML = this.renderExplorer();
          this.bindExplorer(content);
          break;
        case "scm":
          content.innerHTML = this.renderScm();
          this.bindScm(content);
          break;
        case "extensions":
          content.innerHTML = this.renderExtensions();
          this.bindExtensions(content);
          break;
        case "settings":
          content.innerHTML = this.renderSettings();
          this.bindSettings(content);
          break;
        case "workflow":
          content.innerHTML = this.renderWorkflow();
          this.bindWorkflow(content);
          break;
      }
    }
    renderChat() {
      const state = this.workbenchService.state;
      const effectiveModel = state.autoRoute?.model || state.effectiveModel || state.llmForm.model || "";
      const lastUsage = state.lastUsage;
      const sessionUsage = state.sessionUsage;
      const inferencePanel = `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Inference</span></div>
        <div class="kv-list">
          <div class="kv-row"><span>Route</span><strong>${escapeHtml(state.autoRoute?.display_name || effectiveModel || "n/a")}</strong></div>
          <div class="kv-row"><span>Last turn</span><strong>${lastUsage ? estimateCost(effectiveModel, lastUsage.prompt_tokens, lastUsage.completion_tokens, lastUsage.cached_tokens || 0) : "n/a"}</strong></div>
          <div class="kv-row"><span>Last cache hit</span><strong>${lastUsage ? hitRate(lastUsage.prompt_tokens, lastUsage.cached_tokens || 0) : "0%"}</strong></div>
          <div class="kv-row"><span>Session cost</span><strong>${sessionUsage ? estimateCost(effectiveModel, sessionUsage.prompt_tokens, sessionUsage.completion_tokens, sessionUsage.cached_tokens || 0) : "n/a"}</strong></div>
          <div class="kv-row"><span>Session cache hit</span><strong>${sessionUsage ? hitRate(sessionUsage.prompt_tokens, sessionUsage.cached_tokens || 0) : "0%"}</strong></div>
          <div class="kv-row"><span>Turns</span><strong>${sessionUsage?.turns || 0}</strong></div>
        </div>
      </section>`;
      const checkpointSessionIndex = state.checkpointPanelSessionIndex || state.currentSessionIndex;
      const checkpointRowsSource = state.checkpointPanelEntries.length > 0 || checkpointSessionIndex !== state.currentSessionIndex ? state.checkpointPanelEntries : state.currentSessionCheckpoints;
      const sessionRows = state.sessions.length === 0 ? '<div class="empty-state">No saved sessions yet.</div>' : state.sessions.map(
        (session) => `
                <div class="list-row list-row--card${session.current ? " is-active" : ""}">
                  <div>
                    <strong>#${session.index}</strong>
                    <div class="muted">${escapeHtml(session.relative_time)} \xB7 ${session.rounds} rounds \xB7 ${session.checkpoint_count || 0} checkpoints</div>
                  </div>
                  <span class="list-row__preview">${escapeHtml(sanitizePreview(session.preview || ""))}</span>
                  <div class="inline-actions">
                    <button class="text-button" data-inspect-session="${session.index}">Points</button>
                    <button class="text-button" data-restore-session="${session.index}">Restore</button>
                    <button class="text-button" data-fork-session="${session.index}">Fork</button>
                    <button class="text-button danger" data-delete-session="${session.index}">Delete</button>
                  </div>
                </div>`
      ).join("");
      const checkpointRows = checkpointRowsSource.length === 0 ? '<div class="empty-state">Select a session or start a task to inspect restore points.</div>' : checkpointRowsSource.map(
        (checkpoint) => `
                <div class="list-row list-row--card${checkpointSessionIndex === state.currentSessionIndex && checkpoint.index === state.currentSessionActiveCheckpoint ? " is-active" : ""}">
                  <div>
                    <strong>Checkpoint ${checkpoint.index}</strong>
                    <div class="muted">${escapeHtml(checkpoint.relative_time)} \xB7 ${checkpoint.rounds} rounds</div>
                  </div>
                  <span class="list-row__preview">${escapeHtml(sanitizePreview(checkpoint.preview || ""))}</span>
                  <div class="inline-actions">
                    <button class="text-button" data-restore-checkpoint="${checkpoint.index}">Restore</button>
                    <button class="text-button" data-fork-checkpoint="${checkpoint.index}">Fork</button>
                  </div>
                </div>`
      ).join("");
      return `
      ${inferencePanel}
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Sessions</span><button class="text-button" data-new-chat="1">New</button></div>
        <div class="sidebar-list">${sessionRows}</div>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Restore Points</span><span class="muted">${checkpointSessionIndex ? `Session #${checkpointSessionIndex}` : "No active session"}</span></div>
        <div class="sidebar-list">${checkpointRows}</div>
      </section>`;
    }
    bindChat(container) {
      container.querySelector('[data-new-chat="1"]')?.addEventListener("click", () => {
        void this.workbenchService.sendPrompt("/new");
      });
      container.querySelectorAll("[data-inspect-session]").forEach((button) => {
        button.addEventListener("click", () => {
          void this.workbenchService.inspectSessionCheckpoints(Number(button.dataset.inspectSession));
        });
      });
      container.querySelectorAll("[data-restore-session]").forEach((button) => {
        button.addEventListener("click", () => {
          void this.workbenchService.restoreSession(Number(button.dataset.restoreSession));
        });
      });
      container.querySelectorAll("[data-fork-session]").forEach((button) => {
        button.addEventListener("click", () => {
          void this.workbenchService.forkSession(Number(button.dataset.forkSession));
        });
      });
      container.querySelectorAll("[data-delete-session]").forEach((button) => {
        button.addEventListener("click", () => {
          const index = Number(button.dataset.deleteSession);
          const isCurrent = this.workbenchService.state.currentSessionIndex === index;
          const confirmed = window.confirm(
            isCurrent ? `Delete current session #${index}? This also clears the current chat view.` : `Delete session #${index}?`
          );
          if (confirmed) {
            void this.workbenchService.deleteSession(index);
          }
        });
      });
      container.querySelectorAll("[data-restore-checkpoint]").forEach((button) => {
        button.addEventListener("click", () => {
          const checkpoint = Number(button.dataset.restoreCheckpoint);
          const sessionIndex = this.workbenchService.state.checkpointPanelSessionIndex || this.workbenchService.state.currentSessionIndex;
          if (sessionIndex) {
            void this.workbenchService.restoreSession(sessionIndex, checkpoint);
          }
        });
      });
      container.querySelectorAll("[data-fork-checkpoint]").forEach((button) => {
        button.addEventListener("click", () => {
          const checkpoint = Number(button.dataset.forkCheckpoint);
          const sessionIndex = this.workbenchService.state.checkpointPanelSessionIndex || this.workbenchService.state.currentSessionIndex;
          if (sessionIndex) {
            void this.workbenchService.forkSession(sessionIndex, checkpoint);
          }
        });
      });
    }
    renderExplorer() {
      const state = this.workbenchService.state;
      const collapsedPaths = new Set(state.workspaceCollapsedPaths);
      const visibleEntries = this.getVisibleWorkspaceEntries(state.workspaceTree, collapsedPaths);
      const rows = visibleEntries.length === 0 ? '<div class="empty-state">No workspace open.</div>' : visibleEntries.map((entry) => {
        const isCollapsed = entry.type === "dir" && collapsedPaths.has(entry.path);
        return `
                <button
                  class="tree-row tree-row--${entry.type}${isCollapsed ? " is-collapsed" : ""}"
                  data-path="${escapeHtml(entry.path)}"
                  ${entry.type === "dir" ? 'data-tree-folder="1"' : ""}
                  style="--depth:${entry.depth}"
                  ${entry.type === "dir" ? `aria-expanded="${isCollapsed ? "false" : "true"}"` : ""}
                >
                  <span class="tree-row__twistie${entry.type === "file" ? " tree-row__twistie--placeholder" : ""}">
                    ${entry.type === "dir" ? `<i class="codicon codicon-${isCollapsed ? "chevron-right" : "chevron-down"}"></i>` : ""}
                  </span>
                  <i class="codicon codicon-${entry.type === "dir" ? isCollapsed ? "folder" : "folder-opened" : "file"}"></i>
                  <span class="tree-row__label">${escapeHtml(entry.name)}</span>
                </button>`;
      }).join("");
      return `
      <section class="sidebar-section">
        <div class="sidebar-section__header">
          <span>${escapeHtml(state.workspace.active?.path || "Workspace")}</span>
          <button class="text-button" data-refresh-tree="1">Refresh</button>
        </div>
        <div class="tree-list">${rows}</div>
      </section>`;
    }
    bindExplorer(container) {
      container.querySelector('[data-refresh-tree="1"]')?.addEventListener("click", () => {
        void this.workbenchService.refreshAll();
      });
      container.querySelectorAll('[data-tree-folder="1"]').forEach((button) => {
        button.addEventListener("click", () => {
          const path = button.dataset.path;
          if (path) {
            this.workbenchService.toggleWorkspaceFolder(path);
          }
        });
      });
      container.querySelectorAll(".tree-row--file").forEach((button) => {
        button.addEventListener("click", () => {
          const path = button.dataset.path;
          if (path) {
            void this.workbenchService.openPreviewTab(path);
          }
        });
      });
    }
    getVisibleWorkspaceEntries(entries, collapsedPaths) {
      const visibleEntries = [];
      const collapsedDepths = [];
      for (const entry of entries) {
        while (collapsedDepths.length > 0 && entry.depth <= collapsedDepths[collapsedDepths.length - 1]) {
          collapsedDepths.pop();
        }
        if (collapsedDepths.length > 0) {
          continue;
        }
        visibleEntries.push(entry);
        if (entry.type === "dir" && collapsedPaths.has(entry.path)) {
          collapsedDepths.push(entry.depth);
        }
      }
      return visibleEntries;
    }
    // ── Workflow view ─────────────────────────────────────────────────────────
    renderWorkflow() {
      const state = this.workbenchService.state;
      const nodes = state.workflowNodes;
      const active = state.workflowActive;
      const currentNode = state.workflowCurrentNode;
      const MODE_META = {
        work: { icon: "tools", label: "Work", description: "Implement & execute tasks", color: "#4ec9b0" },
        plan: { icon: "list-ordered", label: "Plan", description: "Explore & design without touching code", color: "#9cdcfe" },
        review: { icon: "eye", label: "Review", description: "Audit code for issues & suggest fixes", color: "#ce9178" }
      };
      const statusBanner = active ? `<div class="wf-status wf-status--active">
           <i class="codicon codicon-run-all"></i>
           <span>Workflow running \u2014 step ${currentNode + 1} of ${nodes.length}</span>
         </div>` : nodes.length ? `<div class="wf-status wf-status--idle">
             <i class="codicon codicon-check-all"></i>
             <span>${nodes.length}-step pipeline ready</span>
           </div>` : "";
      const nodeCards = nodes.map((node, i) => {
        const meta = MODE_META[node.mode] || MODE_META.work;
        const isDone = active && i < currentNode;
        const isCurrent = active && i === currentNode;
        return `
          <div class="wf-node${isDone ? " wf-node--done" : isCurrent ? " wf-node--active" : ""}" data-node-index="${i}">
            <div class="wf-node__header">
              <span class="wf-node__badge" style="--node-color:${meta.color}">
                <i class="codicon codicon-${isDone ? "check" : meta.icon}"></i>
              </span>
              <div class="wf-node__meta">
                <select class="wf-node__mode-select select-inline" data-node-mode="${i}">
                  <option value="work"${node.mode === "work" ? " selected" : ""}>Work</option>
                  <option value="plan"${node.mode === "plan" ? " selected" : ""}>Plan</option>
                  <option value="review"${node.mode === "review" ? " selected" : ""}>Review</option>
                </select>
                <span class="wf-node__desc muted">${meta.description}</span>
              </div>
              <div class="wf-node__actions">
                <button class="icon-button" data-node-up="${i}" title="Move up"${i === 0 ? " disabled" : ""}><i class="codicon codicon-arrow-up"></i></button>
                <button class="icon-button" data-node-down="${i}" title="Move down"${i === nodes.length - 1 ? " disabled" : ""}><i class="codicon codicon-arrow-down"></i></button>
                <button class="icon-button danger" data-node-remove="${i}" title="Remove"><i class="codicon codicon-trash"></i></button>
              </div>
            </div>
            <input
              class="text-input wf-node__label"
              data-node-label="${i}"
              placeholder="Optional step label\u2026"
              value="${escapeHtml(node.label || "")}"
            />
          </div>
          ${i < nodes.length - 1 ? '<div class="wf-connector"><i class="codicon codicon-arrow-down"></i></div>' : ""}`;
      }).join("");
      const canAdd = nodes.length < 3;
      return `
      <section class="sidebar-section">
        <div class="sidebar-section__header">
          <span>Pipeline</span>
          <button class="text-button" data-wf-reset="1" title="Clear and reset workflow">Reset</button>
        </div>
        ${statusBanner}
        <div class="wf-pipeline" id="wf-pipeline">
          ${nodeCards || '<div class="wf-empty"><i class="codicon codicon-symbol-misc"></i><span>No steps \u2014 add one below</span></div>'}
        </div>
        ${canAdd ? `
        <div class="wf-add-row">
          <span class="muted">Add step:</span>
          <button class="wf-add-btn" data-wf-add="work"><i class="codicon codicon-tools"></i> Work</button>
          <button class="wf-add-btn" data-wf-add="plan"><i class="codicon codicon-list-ordered"></i> Plan</button>
          <button class="wf-add-btn" data-wf-add="review"><i class="codicon codicon-eye"></i> Review</button>
        </div>` : '<div class="muted wf-limit-note"><i class="codicon codicon-info"></i> Maximum 3 steps reached</div>'}
      </section>
      <div class="wf-footer">
        <button class="primary-button wf-save-btn" data-wf-save="1"${nodes.length === 0 ? " disabled" : ""}>
          <i class="codicon codicon-save"></i> Save & Activate
        </button>
      </div>`;
    }
    bindWorkflow(container) {
      const buildNodes = () => {
        const modeSelects = container.querySelectorAll("[data-node-mode]");
        const labelInputs = container.querySelectorAll("[data-node-label]");
        const result = [];
        modeSelects.forEach((select, i) => {
          result.push({
            mode: select.value,
            label: labelInputs[i]?.value.trim() || ""
          });
        });
        return result;
      };
      container.querySelectorAll("[data-wf-add]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const mode = btn.dataset.wfAdd;
          const current = buildNodes();
          const updated = [...current, { mode, label: "" }];
          void this.workbenchService.saveWorkflow(updated);
        });
      });
      container.querySelectorAll("[data-node-remove]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const idx = Number(btn.dataset.nodeRemove);
          const current = buildNodes();
          current.splice(idx, 1);
          void this.workbenchService.saveWorkflow(current);
        });
      });
      container.querySelectorAll("[data-node-up]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const idx = Number(btn.dataset.nodeUp);
          if (idx === 0) return;
          const current = buildNodes();
          [current[idx - 1], current[idx]] = [current[idx], current[idx - 1]];
          void this.workbenchService.saveWorkflow(current);
        });
      });
      container.querySelectorAll("[data-node-down]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const idx = Number(btn.dataset.nodeDown);
          const current = buildNodes();
          if (idx >= current.length - 1) return;
          [current[idx], current[idx + 1]] = [current[idx + 1], current[idx]];
          void this.workbenchService.saveWorkflow(current);
        });
      });
      container.querySelector('[data-wf-save="1"]')?.addEventListener("click", () => {
        const nodes = buildNodes();
        void this.workbenchService.saveWorkflow(nodes);
      });
      container.querySelector('[data-wf-reset="1"]')?.addEventListener("click", () => {
        if (window.confirm("Clear the current workflow pipeline?")) {
          void this.workbenchService.resetWorkflow();
        }
      });
    }
    renderScm() {
      const state = this.workbenchService.state;
      const rows = state.changes.length === 0 ? '<div class="empty-state">No tracked changes.</div>' : state.changes.map(
        (change) => `
                <div class="change-row">
                  <button class="change-row__main" data-diff-path="${escapeHtml(change.path)}">
                    <strong>${escapeHtml(change.basename)}</strong>
                    <div class="muted">${escapeHtml(change.backup_time)}</div>
                  </button>
                  <button class="icon-button" data-revert-path="${escapeHtml(change.path)}" title="Revert"><i class="codicon codicon-discard"></i></button>
                </div>`
      ).join("");
      return `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Changes</span><button class="text-button" data-refresh-changes="1">Refresh</button></div>
        <div class="sidebar-list">${rows}</div>
      </section>`;
    }
    bindScm(container) {
      container.querySelector('[data-refresh-changes="1"]')?.addEventListener("click", () => {
        void this.workbenchService.refreshAll();
      });
      container.querySelectorAll("[data-diff-path]").forEach((button) => {
        button.addEventListener("click", () => {
          const path = button.dataset.diffPath;
          if (path) {
            void this.workbenchService.openDiffTab(path);
          }
        });
      });
      container.querySelectorAll("[data-revert-path]").forEach((button) => {
        button.addEventListener("click", () => {
          const path = button.dataset.revertPath;
          if (path && window.confirm(`Revert ${path}?`)) {
            void this.workbenchService.revertFile(path);
          }
        });
      });
    }
    renderExtensions() {
      const state = this.workbenchService.state;
      const skills = Array.isArray(state.skills) ? state.skills : [];
      const cards = skills.length === 0 ? '<div class="empty-state">No skills installed.</div>' : skills.map(
        (skill) => `
                <div class="extension-card">
                  <div class="extension-card__title">
                    <strong>${escapeHtml(skill.display_name || skill.name)}</strong>
                    <span class="badge${skill.enabled ? " is-highlight" : ""}">${skill.enabled ? "Enabled" : "Disabled"}</span>
                  </div>
                  <div class="muted">${escapeHtml(skill.description || "No description")}</div>
                  <div class="extension-card__actions">
                    <button class="text-button" data-preview-skill="${escapeHtml(skill.name)}">Preview</button>
                    <button class="text-button" data-toggle-skill="${escapeHtml(skill.name)}">${skill.enabled ? "Disable" : "Enable"}</button>
                    <button class="text-button" data-upgrade-skill="${escapeHtml(skill.name)}">Upgrade</button>
                    <button class="text-button danger" data-delete-skill="${escapeHtml(skill.name)}">Delete</button>
                  </div>
                </div>`
      ).join("");
      return `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Install Skill</span></div>
        <div class="input-group">
          <input id="skill-url-input" class="text-input" placeholder="GitHub or Clawhub URL" />
          <button id="install-skill-button" class="primary-button">Install</button>
        </div>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Installed Skills</span></div>
        <div class="sidebar-list">${cards}</div>
      </section>`;
    }
    bindExtensions(container) {
      container.querySelector("#install-skill-button")?.addEventListener("click", () => {
        const input = container.querySelector("#skill-url-input");
        void this.workbenchService.installSkill(input?.value || "");
      });
      container.querySelectorAll("[data-toggle-skill]").forEach((button) => {
        button.addEventListener("click", () => {
          const name = button.dataset.toggleSkill;
          if (name) {
            void this.workbenchService.toggleSkill(name);
          }
        });
      });
      container.querySelectorAll("[data-preview-skill]").forEach((button) => {
        button.addEventListener("click", () => {
          const name = button.dataset.previewSkill;
          if (name) {
            void this.workbenchService.previewSkill(name);
          }
        });
      });
      container.querySelectorAll("[data-upgrade-skill]").forEach((button) => {
        button.addEventListener("click", () => {
          const name = button.dataset.upgradeSkill;
          if (name) {
            void this.workbenchService.upgradeSkill(name);
          }
        });
      });
      container.querySelectorAll("[data-delete-skill]").forEach((button) => {
        button.addEventListener("click", () => {
          const name = button.dataset.deleteSkill;
          if (name && window.confirm(`Delete skill "${name}"?`)) {
            void this.workbenchService.deleteSkill(name);
          }
        });
      });
    }
    renderSettings() {
      const state = this.workbenchService.state;
      const themeOptions = THEME_OPTIONS.map(
        (theme) => `<option value="${theme}"${theme === state.theme ? " selected" : ""}>${theme}</option>`
      ).join("");
      const modelOptions = state.models.map((model, index) => `<option value="${index}"${index === state.currentModelIndex ? " selected" : ""}>${escapeHtml(model.label || model.model)}</option>`).join("");
      const remote = { ...DEFAULT_REMOTE_FORM, ...state.remote.form || {} };
      const selectedProfileId = matchProviderProfileId(state);
      const selectedProfile = state.providerProfiles.find((profile) => profile.id === selectedProfileId);
      const providerProfiles = state.providerProfiles.map(
        (profile) => `<option value="${escapeHtml(profile.id)}"${profile.id === selectedProfileId ? " selected" : ""}>${escapeHtml(profile.label)} \xB7 ${escapeHtml(profile.model)}</option>`
      ).join("");
      return `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Appearance</span></div>
        <label class="field"><span>Theme</span><select id="theme-select" class="text-input">${themeOptions}</select></label>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Model</span></div>
        <label class="field"><span>Current model</span><select id="model-select" class="text-input">${modelOptions}</select></label>
        <label class="field"><span>DeepSeek preset</span><select id="provider-profile-select" class="text-input">${providerProfiles}</select></label>
        <div class="muted">Profiles prefill official, managed, and self-hosted DeepSeek-compatible endpoints.</div>
        ${selectedProfile ? `<div class="profile-card"><strong>${escapeHtml(selectedProfile.label)}</strong><div class="muted">${escapeHtml(selectedProfile.description)}</div><div class="muted">${escapeHtml(selectedProfile.apibase)} \xB7 ${escapeHtml(selectedProfile.model)}</div></div>` : ""}
        <button id="apply-profile-button" class="text-button">Apply preset</button>
        <label class="field"><span>Provider</span><input id="provider-input" class="text-input" value="${escapeHtml(state.llmForm.provider || "")}" /></label>
        <label class="field"><span>Display name</span><input id="display-name-input" class="text-input" value="${escapeHtml(state.llmForm.name || "")}" /></label>
        <label class="field"><span>Model name</span><input id="model-name-input" class="text-input" value="${escapeHtml(state.llmForm.model || "")}" /></label>
        <label class="field"><span>Base URL</span><input id="base-url-input" class="text-input" value="${escapeHtml(state.llmForm.apibase || "")}" title="The API endpoint URL, e.g. https://api.openai.com/v1" /></label>
        <div class="field api-key-group">
          <span>API Key</span>
          <div class="input-group">
            <input id="api-key-input" class="text-input" type="password" value="${escapeHtml(state.llmForm.apikey || "")}" autocomplete="off" />
            <button type="button" id="api-key-toggle" class="api-key-toggle text-button" title="Show/hide API key">\u{1F441}</button>
          </div>
        </div>
        <button id="save-model-button" class="primary-button">Save model settings</button>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Workspace</span></div>
        <label class="field"><span>Name</span><input id="workspace-name-input" class="text-input" value="${escapeHtml(state.workspaceDraftName)}" /></label>
        <div class="input-group">
          <input id="workspace-path-input" class="text-input" value="${escapeHtml(state.workspaceDraftPath)}" placeholder="/path/to/workspace" />
          <button id="pick-workspace-button" class="text-button">Browse</button>
        </div>
        <button id="save-workspace-button" class="primary-button">Open workspace</button>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Remote</span></div>
        <label class="toggle-inline"><input id="remote-enabled-input" type="checkbox"${remote.enabled ? " checked" : ""} />Enable SSH</label>
        <label class="field"><span>Server name</span><input id="remote-name-input" class="text-input" value="${escapeHtml(remote.server_name || remote.name || "")}" /></label>
        <label class="field"><span>Host</span><input id="remote-host-input" class="text-input" value="${escapeHtml(remote.host)}" /></label>
        <label class="field"><span>Port</span><input id="remote-port-input" class="text-input" value="${remote.port}" /></label>
        <label class="field"><span>Username</span><input id="remote-user-input" class="text-input" value="${escapeHtml(remote.username)}" /></label>
        <label class="field"><span>Password</span><input id="remote-password-input" class="text-input" type="password" value="${escapeHtml(remote.password)}" /></label>
        <label class="field"><span>Key path</span><input id="remote-key-input" class="text-input" value="${escapeHtml(remote.key_path)}" /></label>
        <label class="field"><span>Working dir</span><input id="remote-cwd-input" class="text-input" value="${escapeHtml(remote.cwd)}" /></label>
        <button id="save-remote-button" class="primary-button">${state.remote.connected ? "Reconnect remote" : "Connect remote"}</button>
      </section>`;
    }
    bindSettings(container) {
      container.querySelector("#theme-select")?.addEventListener("change", (event) => {
        this.workbenchService.setTheme(event.currentTarget.value);
      });
      container.querySelector("#model-select")?.addEventListener("change", (event) => {
        void this.workbenchService.switchModel(Number(event.currentTarget.value));
      });
      container.querySelector("#apply-profile-button")?.addEventListener("click", () => {
        const select = container.querySelector("#provider-profile-select");
        void this.workbenchService.applyProviderProfile(select.value);
      });
      container.querySelector("#save-model-button")?.addEventListener("click", () => {
        void this.workbenchService.saveModelSettings({
          provider: container.querySelector("#provider-input").value.trim(),
          name: container.querySelector("#display-name-input").value.trim(),
          model: container.querySelector("#model-name-input").value.trim(),
          apibase: container.querySelector("#base-url-input").value.trim(),
          apikey: container.querySelector("#api-key-input").value.trim()
        });
      });
      container.querySelector("#api-key-toggle")?.addEventListener("click", () => {
        const apiKeyInput = container.querySelector("#api-key-input");
        const toggleBtn = container.querySelector("#api-key-toggle");
        if (apiKeyInput.type === "password") {
          apiKeyInput.type = "text";
          toggleBtn.textContent = "\u{1F648}";
          toggleBtn.title = "Hide API key";
        } else {
          apiKeyInput.type = "password";
          toggleBtn.textContent = "\u{1F441}";
          toggleBtn.title = "Show API key";
        }
      });
      container.querySelector("#save-workspace-button")?.addEventListener("click", () => {
        void this.workbenchService.saveWorkspaceSettings({
          name: container.querySelector("#workspace-name-input").value.trim(),
          path: container.querySelector("#workspace-path-input").value.trim()
        });
      });
      container.querySelector("#workspace-name-input")?.addEventListener("input", (event) => {
        this.workbenchService.setWorkspaceDraft(
          event.currentTarget.value,
          container.querySelector("#workspace-path-input").value
        );
      });
      container.querySelector("#workspace-path-input")?.addEventListener("input", (event) => {
        this.workbenchService.setWorkspaceDraft(
          container.querySelector("#workspace-name-input").value,
          event.currentTarget.value
        );
      });
      container.querySelector("#pick-workspace-button")?.addEventListener("click", async () => {
        const picked = await this.workbenchService.pickWorkspacePath();
        if (picked) {
          const pathInput = container.querySelector("#workspace-path-input");
          const nameInput = container.querySelector("#workspace-name-input");
          pathInput.value = picked;
          nameInput.value = picked.split(/[\\/]/).filter(Boolean).pop() || "";
          this.workbenchService.setWorkspaceDraft(nameInput.value, pathInput.value);
        }
      });
      container.querySelector("#save-remote-button")?.addEventListener("click", () => {
        void this.workbenchService.saveRemoteSettings({
          enabled: container.querySelector("#remote-enabled-input").checked,
          server_name: container.querySelector("#remote-name-input").value.trim(),
          host: container.querySelector("#remote-host-input").value.trim(),
          port: Number(container.querySelector("#remote-port-input").value || "22"),
          username: container.querySelector("#remote-user-input").value.trim() || "root",
          password: container.querySelector("#remote-password-input").value,
          key_path: container.querySelector("#remote-key-input").value.trim(),
          cwd: container.querySelector("#remote-cwd-input").value.trim()
        });
      });
    }
  };

  // workbench/src/workbench/parts/statusbarPart.ts
  var MODEL_PRICING2 = {
    "deepseek-v4-pro": { cacheHit: 3625e-6, cacheMiss: 0.435, output: 0.87 },
    "deepseek-v4-flash": { cacheHit: 28e-4, cacheMiss: 0.14, output: 0.28 },
    "deepseek-reasoner": { cacheHit: 3625e-6, cacheMiss: 0.435, output: 0.87 },
    "deepseek-chat": { cacheHit: 28e-4, cacheMiss: 0.14, output: 0.28 }
  };
  function estimateCost2(model, promptTokens, completionTokens, cachedTokens) {
    const lm = model.toLowerCase();
    const pricing = Object.entries(MODEL_PRICING2).find(([k]) => lm.includes(k))?.[1];
    if (!pricing) return "";
    const missTokens = promptTokens - cachedTokens;
    const cost = cachedTokens / 1e6 * pricing.cacheHit + missTokens / 1e6 * pricing.cacheMiss + completionTokens / 1e6 * pricing.output;
    return cost < 1e-3 ? `<$0.001` : `$${cost.toFixed(4)}`;
  }
  var StatusbarPart = class extends Disposable {
    layoutService;
    workbenchService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
      this.workbenchService = accessor.get(IWorkbenchService);
      this._register(this.workbenchService.onDidChangeState(() => this.render()));
      this.render();
    }
    render() {
      const state = this.workbenchService.state;
      const effectiveModel = state.autoRoute?.model || state.modelLabel;
      this.layoutService.getElement("status-model").textContent = state.autoModelEnabled ? state.autoRoute ? `Auto -> ${state.autoRoute.model}${state.autoRoute.reasoning_effort ? ` / ${state.autoRoute.reasoning_effort}` : ""}` : `Auto (${state.modelLabel})` : state.modelLabel;
      this.layoutService.getElement("status-workspace").textContent = state.workspace.active?.path || "No workspace";
      this.layoutService.getElement("status-mode").textContent = state.currentMode.toUpperCase();
      this.layoutService.getElement("status-plan").textContent = state.planRemaining >= 0 ? `Plan: ${state.planRemaining} remaining` : "Plan: idle";
      this.layoutService.getElement("status-run").textContent = state.isRunning ? "Running" : "Idle";
      const usageEl = this.layoutService.getElement("status-usage");
      if (state.lastUsage) {
        const { prompt_tokens, completion_tokens, cached_tokens } = state.lastUsage;
        const cached = cached_tokens ?? 0;
        const costStr = estimateCost2(effectiveModel, prompt_tokens, completion_tokens, cached);
        const tokStr = `\u2191${prompt_tokens} \u2193${completion_tokens}${cached ? ` \u{1F4BE}${cached}` : ""}`;
        usageEl.textContent = costStr ? `${tokStr} ${costStr}` : tokStr;
        usageEl.title = `Prompt: ${prompt_tokens} | Completion: ${completion_tokens} | Cached: ${cached}`;
      } else {
        usageEl.textContent = "";
        usageEl.title = "";
      }
    }
  };

  // workbench/src/workbench/services/commandService.ts
  var CommandService = class {
    constructor(accessor) {
      this.accessor = accessor;
    }
    commands = /* @__PURE__ */ new Map();
    registerCommand(command) {
      this.commands.set(command.id, command);
    }
    getCommands() {
      return [...this.commands.values()];
    }
    async executeCommand(id) {
      const command = this.commands.get(id);
      if (!command) {
        throw new Error(`Unknown command: ${id}`);
      }
      await command.run();
    }
    registerCoreCommands() {
      const workbench = this.accessor.get(IWorkbenchService);
      const openView = (view) => () => workbench.setActiveView(view);
      this.registerCommand({ id: "workbench.action.quickOpen", label: "Quick Open", run: () => workbench.setQuickOpenVisible(true) });
      this.registerCommand({ id: "workbench.action.closeQuickOpen", label: "Close Quick Open", run: () => workbench.setQuickOpenVisible(false) });
      this.registerCommand({ id: "workbench.action.newChat", label: "New Chat", run: () => workbench.sendPrompt("/new") });
      this.registerCommand({ id: "workbench.action.stop", label: "Stop", run: () => workbench.stopTask() });
      this.registerCommand({ id: "workbench.action.refresh", label: "Refresh", run: () => workbench.refreshAll() });
      this.registerCommand({ id: "workbench.action.toggleSidebar", label: "Toggle Sidebar", run: () => workbench.toggleSidebar() });
      this.registerCommand({ id: "workbench.view.chat", label: "Open Assistant View", run: openView("chat") });
      this.registerCommand({ id: "workbench.view.explorer", label: "Open Explorer View", run: openView("explorer") });
      this.registerCommand({ id: "workbench.view.changes", label: "Open Changes View", run: openView("scm") });
      this.registerCommand({ id: "workbench.view.extensions", label: "Open Skills View", run: openView("extensions") });
      this.registerCommand({ id: "workbench.view.settings", label: "Open Settings View", run: openView("settings") });
    }
  };

  // workbench/src/workbench/services/layoutService.ts
  var LayoutService = class {
    constructor(root2) {
      this.root = root2;
    }
    renderShell() {
      this.root.innerHTML = `
      <div class="workbench">
        <header class="titlebar">
          <div class="titlebar__brand">
            <img src="/static/icon.ico" alt="" />
            <span>Generic Coder Workbench</span>
          </div>
          <div class="titlebar__actions">
            <button class="icon-button" data-command="workbench.action.quickOpen" title="Quick Open"><i class="codicon codicon-search"></i></button>
            <button class="icon-button" data-command="workbench.action.newChat" title="New Chat"><i class="codicon codicon-add"></i></button>
            <button class="icon-button" data-command="workbench.action.stop" title="Stop"><i class="codicon codicon-debug-stop"></i></button>
            <button class="icon-button" data-command="workbench.action.refresh" title="Refresh"><i class="codicon codicon-refresh"></i></button>
          </div>
        </header>
        <div class="workbench__body">
          <nav class="activitybar" id="activitybar"></nav>
          <aside class="sidebar">
            <div class="sidebar__header">
              <span id="sidebar-title">Assistant</span>
              <button class="icon-button" data-command="workbench.action.toggleSidebar" title="Toggle View"><i class="codicon codicon-layout-sidebar-left"></i></button>
            </div>
            <div class="sidebar__content" id="sidebar-content"></div>
          </aside>
          <main class="editor-area">
            <div class="tabs" id="editor-tabs"></div>
            <section class="editor-surface" id="editor-surface"></section>
            <section class="composer">
              <div class="composer__toolbar">
                <select id="mode-select" class="select-inline">
                  <option value="work">Work</option>
                  <option value="plan">Plan</option>
                  <option value="review">Review</option>
                </select>
                <label class="toggle-inline"><input type="checkbox" id="multi-agent-toggle" />Multi-Agent</label>
                <label class="toggle-inline"><input type="checkbox" id="one-shot-toggle" />One Shot</label>
                <label class="toggle-inline toggle-inline--loop"><input type="checkbox" id="loop-toggle" disabled />Loop</label>
                <label class="toggle-inline toggle-inline--workflow"><input type="checkbox" id="workflow-follow-toggle" /><span id="workflow-follow-label">Workflow</span></label>
                <label class="toggle-inline toggle-inline--computer-use"><input type="checkbox" id="computer-use-toggle" />Computer Use</label>
                <label class="toggle-inline toggle-inline--yolo"><input type="checkbox" id="yolo-toggle" />YOLO</label>
                <label class="toggle-inline toggle-inline--auto-model"><input type="checkbox" id="auto-model-toggle" />Auto Model</label>
                <div class="effort-group" id="effort-group">
                  <span class="effort-label">Effort:</span>
                  <button class="effort-btn" data-effort="off">Off</button>
                  <button class="effort-btn" data-effort="high">High</button>
                  <button class="effort-btn" data-effort="max">Max</button>
                </div>
                <span class="composer__meta" id="composer-meta"></span>
              </div>
              <div id="yolo-warning" class="yolo-warning" hidden>
                <i class="codicon codicon-warning"></i>
                <span class="yolo-warning__text">YOLO mode is active \u2014 AI will execute all actions without asking for confirmation</span>
                <button id="yolo-warning-off" class="yolo-warning__off">Turn off</button>
              </div>
              <div class="composer__body">
                <div class="composer__input-wrap">
                  <div id="slash-hints" class="slash-hints" hidden></div>
                  <textarea id="prompt-input" spellcheck="false" placeholder="Describe the task\u2026 Type / for commands, @ to mention a file."></textarea>
                </div>
                <button id="send-button" class="primary-button">Send</button>
              </div>
            </section>
          </main>
        </div>
        <footer class="statusbar">
          <div class="statusbar__left">
            <span class="status-pill"><i class="codicon codicon-symbol-misc"></i><span id="status-model">Model offline</span></span>
            <span class="status-pill"><i class="codicon codicon-folder-opened"></i><span id="status-workspace">No workspace</span></span>
            <span class="status-pill"><i class="codicon codicon-play-circle"></i><span id="status-mode">Work</span></span>
          </div>
          <div class="statusbar__right">
            <span class="status-pill" id="status-usage"></span>
            <span class="status-pill" id="status-plan">Plan: idle</span>
            <span class="status-pill" id="status-run">Idle</span>
          </div>
        </footer>
      </div>
      <div class="toast-stack" id="toast-stack"></div>
      <div class="quick-open" id="quick-open" hidden>
        <div class="quick-open__scrim" data-command="workbench.action.closeQuickOpen"></div>
        <div class="quick-open__panel">
          <div class="quick-open__input-wrap">
            <i class="codicon codicon-search"></i>
            <input id="quick-open-input" type="text" placeholder="Search files or run commands" />
          </div>
          <div class="quick-open__results" id="quick-open-results"></div>
        </div>
      </div>
    `;
    }
    getElement(id) {
      const element = document.getElementById(id);
      if (!element) {
        throw new Error(`Missing layout element: ${id}`);
      }
      return element;
    }
    getRoot() {
      return this.root;
    }
  };

  // workbench/src/workbench/services/notificationService.ts
  var NotificationService = class extends Disposable {
    layoutService;
    constructor(accessor) {
      super();
      this.layoutService = accessor.get(ILayoutService);
    }
    notify(message) {
      const stack = this.layoutService.getElement("toast-stack");
      const item = document.createElement("div");
      item.className = "toast";
      item.textContent = message;
      stack.appendChild(item);
      window.setTimeout(() => {
        item.remove();
      }, 3200);
    }
  };

  // workbench/src/api.ts
  async function readJson(response) {
    const payload = await response.json();
    if (!response.ok) {
      throw new Error(payload.error || `HTTP ${response.status}`);
    }
    return payload;
  }
  var ApiClient = class {
    async bootstrap() {
      const response = await fetch("/api/bootstrap");
      return readJson(response);
    }
    async models() {
      const response = await fetch("/api/models");
      return readJson(response);
    }
    async settings() {
      const response = await fetch("/api/settings");
      return readJson(response);
    }
    async setTheme(theme) {
      const response = await fetch("/api/theme", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ theme })
      });
      await readJson(response);
    }
    async setModel(index) {
      const response = await fetch("/api/model", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ index })
      });
      return readJson(response);
    }
    async saveLlmConfig(payload) {
      const response = await fetch("/api/llm-config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload)
      });
      return readJson(response);
    }
    async saveWorkspace(payload) {
      const response = await fetch("/api/workspace", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload)
      });
      return readJson(response);
    }
    async pickerToken() {
      const response = await fetch("/api/workspace/picker-token");
      const payload = await readJson(response);
      return payload.token || "";
    }
    async pickWorkspace(token) {
      const response = await fetch("/api/workspace/pick", {
        method: "POST",
        headers: {
          "X-Generic-Coder-UI": "1",
          "X-Generic-Coder-Picker-Token": token
        }
      });
      return readJson(response);
    }
    async connectRemote(payload) {
      const response = await fetch("/api/remote/connect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload)
      });
      return readJson(response);
    }
    async workspaceTree() {
      const response = await fetch("/api/workspace/tree");
      const payload = await readJson(response);
      return payload.tree || [];
    }
    async workspaceFiles(query, limit = 20) {
      const response = await fetch(`/api/workspace/files?q=${encodeURIComponent(query)}&limit=${limit}`);
      const payload = await readJson(response);
      return payload.files || [];
    }
    async workspacePreview(filePath) {
      const response = await fetch(`/api/workspace/preview?path=${encodeURIComponent(filePath)}`);
      return readJson(response);
    }
    workspacePreviewContentUrl(filePath) {
      return `/api/workspace/preview-content?path=${encodeURIComponent(filePath)}`;
    }
    async sessions() {
      const response = await fetch("/api/sessions");
      const payload = await readJson(response);
      return payload.sessions || [];
    }
    async sessionCheckpoints(index) {
      const response = await fetch(`/api/sessions/${index}/checkpoints`);
      const payload = await readJson(response);
      return payload.checkpoints || [];
    }
    async restoreSession(index, checkpoint) {
      const response = await fetch("/api/sessions/restore", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ index, checkpoint })
      });
      return readJson(response);
    }
    async forkSession(index, checkpoint) {
      const response = await fetch("/api/sessions/fork", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ index, checkpoint })
      });
      return readJson(response);
    }
    async deleteSession(index) {
      const response = await fetch(`/api/sessions/${index}/delete`, { method: "POST" });
      return readJson(response);
    }
    async changes() {
      const response = await fetch("/api/changes");
      const payload = await readJson(response);
      return payload.changes || [];
    }
    async diff(filePath) {
      const response = await fetch("/api/diff", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: filePath })
      });
      return readJson(response);
    }
    async revert(filePath) {
      const response = await fetch("/api/revert", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: filePath })
      });
      await readJson(response);
    }
    async skills() {
      const response = await fetch("/api/skills");
      const payload = await readJson(response);
      return Array.isArray(payload.skills) ? payload.skills : [];
    }
    async installSkill(url) {
      const response = await fetch("/api/skills/install", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url })
      });
      await readJson(response);
    }
    async toggleSkill(name) {
      const response = await fetch(`/api/skills/${encodeURIComponent(name)}/toggle`, { method: "POST" });
      await readJson(response);
    }
    async deleteSkill(name) {
      const response = await fetch(`/api/skills/${encodeURIComponent(name)}/delete`, { method: "POST" });
      await readJson(response);
    }
    async upgradeSkill(name) {
      const response = await fetch(`/api/skills/${encodeURIComponent(name)}/upgrade`, { method: "POST" });
      await readJson(response);
    }
    async previewSkill(name) {
      const response = await fetch(`/api/skills/${encodeURIComponent(name)}/preview`);
      return readJson(response);
    }
    async workflow() {
      const response = await fetch("/api/workflow");
      return readJson(response);
    }
    async saveWorkflow(nodes) {
      const response = await fetch("/api/workflow", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ nodes })
      });
      await readJson(response);
    }
    async resetWorkflow() {
      const response = await fetch("/api/workflow/reset", { method: "POST" });
      await readJson(response);
    }
    async getWorkflowFollow() {
      const response = await fetch("/api/workflow/follow");
      return readJson(response);
    }
    async setWorkflowFollow(enabled) {
      const response = await fetch("/api/workflow/follow", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      return readJson(response);
    }
    async checkLoopSuitable(prompt) {
      const response = await fetch("/api/loop/suitable", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt })
      });
      return readJson(response);
    }
    async setLoop(enabled) {
      const response = await fetch("/api/loop", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      await readJson(response);
    }
    async mode() {
      const response = await fetch("/api/mode");
      const payload = await readJson(response);
      return payload.mode;
    }
    async setMode(mode) {
      const response = await fetch("/api/mode", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ mode })
      });
      await readJson(response);
    }
    async setMultiAgent(enabled) {
      const response = await fetch("/api/multi-agent", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      await readJson(response);
    }
    async checkMultiAgentSuitable(prompt) {
      const response = await fetch("/api/multi-agent/suitable", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt })
      });
      return readJson(response);
    }
    async setOneShot(enabled) {
      const response = await fetch("/api/one-shot", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      await readJson(response);
    }
    async setComputerUse(enabled) {
      const response = await fetch("/api/computer-use", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      return readJson(response);
    }
    async setYolo(enabled) {
      const response = await fetch("/api/yolo", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      return readJson(response);
    }
    async setAutoModel(enabled) {
      const response = await fetch("/api/auto-model", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled })
      });
      return readJson(response);
    }
    async setReasoningEffort(effort) {
      const response = await fetch("/api/reasoning-effort", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ effort })
      });
      return readJson(response);
    }
    async chat(prompt) {
      const response = await fetch("/api/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt })
      });
      return readJson(response);
    }
    async task(taskId) {
      const response = await fetch(`/api/tasks/${taskId}`);
      return readJson(response);
    }
    async stop() {
      const response = await fetch("/api/stop", { method: "POST" });
      await readJson(response);
    }
    async planStatus() {
      const response = await fetch("/api/plan/status");
      return readJson(response);
    }
  };

  // workbench/src/base/common/event.ts
  var Emitter = class extends Disposable {
    listeners = /* @__PURE__ */ new Set();
    event = (listener) => {
      this.listeners.add(listener);
      return toDisposable(() => {
        this.listeners.delete(listener);
      });
    };
    fire(event) {
      for (const listener of [...this.listeners]) {
        listener(event);
      }
    }
  };

  // workbench/src/workbench/services/workbenchService.ts
  var WorkbenchService = class extends Disposable {
    api = new ApiClient();
    stateValue = createInitialWorkbenchState();
    changeEmitter = this._register(new Emitter());
    onDidChangeState = this.changeEmitter.event;
    pollingTaskId = null;
    notifications;
    constructor(accessor) {
      super();
      this.notifications = accessor.get(INotificationService);
    }
    get state() {
      return this.stateValue;
    }
    async start() {
      const storedTheme = window.localStorage.getItem("generic-coder-theme");
      if (storedTheme && THEME_OPTIONS.includes(storedTheme)) {
        this.stateValue.theme = storedTheme;
      }
      this.applyTheme(false);
      await this.hydrateWorkspacePickerToken();
      await this.bootstrap();
      const interval = window.setInterval(() => {
        void this.refreshLightweightState();
      }, 5e3);
      this._register(toDisposable(() => window.clearInterval(interval)));
    }
    ensureTaskPlaceholder(preview) {
      const content = preview || "...";
      if (typeof this.stateValue.taskPlaceholderIndex === "number" && this.stateValue.messages[this.stateValue.taskPlaceholderIndex]) {
        this.stateValue.messages[this.stateValue.taskPlaceholderIndex] = {
          role: "assistant",
          content,
          streaming: true
        };
        return;
      }
      const lastIndex = this.stateValue.messages.length - 1;
      const lastMessage = this.stateValue.messages[lastIndex];
      if (lastMessage?.role === "assistant" && lastMessage.streaming) {
        this.stateValue.messages[lastIndex] = {
          role: "assistant",
          content,
          streaming: true
        };
        this.stateValue.taskPlaceholderIndex = lastIndex;
        return;
      }
      this.stateValue.messages.push({ role: "assistant", content, streaming: true });
      this.stateValue.taskPlaceholderIndex = this.stateValue.messages.length - 1;
    }
    maybeResumePendingTask(data) {
      const pendingTaskId = data.pending_task?.task_id;
      if (!data.is_running || !pendingTaskId || this.pollingTaskId === pendingTaskId) {
        return;
      }
      this.stateValue.pendingTaskId = pendingTaskId;
      this.ensureTaskPlaceholder(data.pending_task?.preview || "Starting task...");
      void this.pollTask(pendingTaskId);
    }
    setActiveView(view) {
      this.stateValue.activeView = view;
      this.emitChange();
    }
    toggleSidebar() {
      this.setActiveView(this.state.activeView === "chat" ? "explorer" : "chat");
    }
    setActiveTab(tabId) {
      this.stateValue.activeTabId = tabId;
      this.emitChange();
    }
    setQuickOpenVisible(visible) {
      this.stateValue.quickOpenVisible = visible;
      this.emitChange();
    }
    setInputValue(value, emit = false) {
      this.stateValue.inputValue = value;
      if (emit) {
        this.emitChange();
      }
    }
    insertMention(filePath, cursor) {
      const beforeCursor = this.stateValue.inputValue.slice(0, cursor ?? this.stateValue.inputValue.length);
      const afterCursor = this.stateValue.inputValue.slice(cursor ?? this.stateValue.inputValue.length);
      const atIndex = beforeCursor.lastIndexOf("@");
      this.stateValue.inputValue = atIndex >= 0 ? `${beforeCursor.slice(0, atIndex)}@${filePath} ${afterCursor}` : `${beforeCursor}@${filePath} ${afterCursor}`;
      this.emitChange();
      return this.stateValue.inputValue;
    }
    async fetchMentionSuggestions(query) {
      if (!query.trim()) {
        return [];
      }
      return this.api.workspaceFiles(query, 5).catch(() => []);
    }
    async getQuickOpenFileResults(query) {
      const fileRows = query ? (await this.api.workspaceFiles(query, 10)).map((item) => ({
        kind: "file",
        label: item.rel || item.name,
        value: item.path
      })) : [];
      return fileRows;
    }
    async refreshAll() {
      await Promise.all([
        this.loadWorkspaceTree(),
        this.loadSessions(),
        this.loadChanges(),
        this.loadSkills(),
        this.loadWorkflowState(),
        this.refreshPlanStatus()
      ]);
      this.emitChange();
    }
    async switchModel(index) {
      try {
        await this.api.setModel(index);
        this.applyModelSettings(await this.api.settings());
        this.emitChange();
        this.notifications.notify(`Switched to ${this.stateValue.modelLabel}`);
      } catch (error) {
        this.notifyError(error, "Failed to switch model");
      }
    }
    async saveModelSettings(payload) {
      try {
        const request = {
          entry_key: this.state.llmForm.entry_key || "",
          session_type: this.state.llmForm.session_type || "native_oai",
          protocol_preset: this.state.llmForm.protocol_preset || "custom",
          api_mode: this.state.llmForm.api_mode || "chat_completions",
          ...payload
        };
        await this.api.saveLlmConfig(request);
        this.applyModelSettings(await this.api.settings());
        this.emitChange();
        this.notifications.notify("Model settings saved");
      } catch (error) {
        this.notifyError(error, "Failed to save model settings");
      }
    }
    async applyProviderProfile(profileId) {
      const profile = this.stateValue.providerProfiles.find((entry) => entry.id === profileId);
      if (!profile) {
        this.notifications.notify("Unknown provider profile");
        return;
      }
      await this.saveModelSettings({
        provider: profile.provider,
        name: profile.label,
        model: profile.model,
        apibase: profile.apibase,
        apikey: this.stateValue.llmForm.apikey || "",
        reasoning_effort: profile.reasoning_effort || null
      });
    }
    async inspectSessionCheckpoints(index) {
      try {
        if (this.stateValue.currentSessionIndex === index) {
          this.stateValue.checkpointPanelSessionIndex = index;
          this.stateValue.checkpointPanelEntries = this.stateValue.currentSessionCheckpoints;
          this.emitChange();
          return;
        }
        this.stateValue.checkpointPanelSessionIndex = index;
        this.stateValue.checkpointPanelEntries = await this.api.sessionCheckpoints(index);
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to load restore points");
      }
    }
    async saveWorkspaceSettings(payload) {
      try {
        this.setWorkspaceDraft(payload.name, payload.path);
        const response = await this.api.saveWorkspace(payload);
        this.stateValue.workspace = {
          active: response.active || null,
          workspaces: response.workspaces || [],
          recent_folders: response.recent_folders || []
        };
        this.syncWorkspaceDraftFromActive();
        await this.loadWorkspaceTree();
        this.emitChange();
        this.notifications.notify("Workspace opened");
      } catch (error) {
        this.notifyError(error, "Failed to save workspace");
      }
    }
    async pickWorkspacePath() {
      if (!this.stateValue.workspacePickerToken) {
        this.notifications.notify("Workspace picker is unavailable");
        return null;
      }
      try {
        const payload = await this.api.pickWorkspace(this.stateValue.workspacePickerToken);
        return payload.path || null;
      } catch (error) {
        this.notifyError(error, "Failed to open picker");
        return null;
      }
    }
    setWorkspaceDraft(name, path) {
      this.stateValue.workspaceDraftName = name;
      this.stateValue.workspaceDraftPath = path;
    }
    toggleWorkspaceFolder(path) {
      const collapsedPaths = new Set(this.stateValue.workspaceCollapsedPaths);
      if (collapsedPaths.has(path)) {
        collapsedPaths.delete(path);
      } else {
        collapsedPaths.add(path);
      }
      this.stateValue.workspaceCollapsedPaths = [...collapsedPaths];
      this.emitChange();
    }
    async saveRemoteSettings(payload) {
      try {
        const remote = await this.api.connectRemote(payload);
        this.stateValue.remote = remote;
        this.emitChange();
        this.notifications.notify(remote.message || "Remote state updated");
      } catch (error) {
        this.notifyError(error, "Failed to update remote");
      }
    }
    async installSkill(url) {
      if (!url.trim()) {
        this.notifications.notify("Enter a skill URL first");
        return;
      }
      try {
        await this.api.installSkill(url.trim());
        await this.loadSkills();
        this.emitChange();
        this.notifications.notify("Skill installed");
      } catch (error) {
        this.notifyError(error, "Failed to install skill");
      }
    }
    async toggleSkill(name) {
      try {
        await this.api.toggleSkill(name);
        await this.loadSkills();
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to toggle skill");
      }
    }
    async upgradeSkill(name) {
      try {
        await this.api.upgradeSkill(name);
        await this.loadSkills();
        this.emitChange();
        this.notifications.notify(`Upgraded ${name}`);
      } catch (error) {
        this.notifyError(error, "Failed to upgrade skill");
      }
    }
    async deleteSkill(name) {
      try {
        await this.api.deleteSkill(name);
        await this.loadSkills();
        this.emitChange();
        this.notifications.notify(`Deleted ${name}`);
      } catch (error) {
        this.notifyError(error, "Failed to delete skill");
      }
    }
    async previewSkill(name) {
      try {
        const preview = await this.api.previewSkill(name);
        this.stateValue.messages.push({
          role: "assistant",
          content: `Skill preview: ${name}

${preview.file || ""}

${preview.content || ""}`
        });
        this.stateValue.activeTabId = "chat";
        this.ensureChatTab();
        this.emitChange();
        this.notifications.notify(`Loaded ${name} preview`);
      } catch (error) {
        this.notifyError(error, "Failed to preview skill");
      }
    }
    async setMode(mode) {
      try {
        await this.api.setMode(mode);
        this.stateValue.currentMode = mode;
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to set mode");
      }
    }
    async toggleMultiAgent(enabled, prompt) {
      if (enabled) {
        if (!prompt.trim()) {
          this.stateValue.multiAgentEnabled = false;
          this.emitChange();
          this.notifications.notify("Type a task before enabling multi-agent");
          return;
        }
        try {
          const suitability = await this.api.checkMultiAgentSuitable(prompt);
          if (!suitability.suitable) {
            this.stateValue.multiAgentEnabled = false;
            this.emitChange();
            this.notifications.notify(suitability.reason || "Task is not suitable for multi-agent");
            return;
          }
        } catch (error) {
          this.stateValue.multiAgentEnabled = false;
          this.emitChange();
          this.notifyError(error, "Failed to enable multi-agent");
          return;
        }
        if (this.stateValue.oneShotEnabled) {
          this.stateValue.oneShotEnabled = false;
          await this.api.setOneShot(false).catch(() => {
          });
        }
      }
      this.stateValue.multiAgentEnabled = enabled;
      await this.api.setMultiAgent(enabled).catch(() => {
      });
      this.emitChange();
    }
    async toggleOneShot(enabled) {
      if (enabled && this.stateValue.multiAgentEnabled) {
        this.stateValue.multiAgentEnabled = false;
        await this.api.setMultiAgent(false).catch(() => {
        });
      }
      this.stateValue.oneShotEnabled = enabled;
      await this.api.setOneShot(enabled).catch(() => {
      });
      this.emitChange();
    }
    async toggleWorkflowFollow(enabled) {
      if (enabled && this.stateValue.workflowNodes.length === 0) {
        this.notifications.notify("No workflow steps configured \u2014 add steps in the Workflow panel first");
        this.stateValue.workflowFollowEnabled = false;
        this.emitChange();
        return;
      }
      const result = await this.api.setWorkflowFollow(enabled).catch(() => ({ ok: false, reason: "API error" }));
      if (!result.ok && enabled) {
        this.notifications.notify(result.reason || "Cannot enable workflow follow");
        this.stateValue.workflowFollowEnabled = false;
      } else {
        this.stateValue.workflowFollowEnabled = enabled;
      }
      this.emitChange();
    }
    async toggleComputerUse(enabled) {
      if (enabled && !this.stateValue.computerUseAvailable) {
        this.notifications.notify("Computer Use is not available on this platform");
        this.stateValue.computerUseEnabled = false;
        this.emitChange();
        return;
      }
      try {
        const result = await this.api.setComputerUse(enabled);
        this.stateValue.computerUseEnabled = result.enabled;
      } catch {
        this.stateValue.computerUseEnabled = false;
      }
      this.emitChange();
    }
    async toggleYolo(enabled) {
      try {
        await this.api.setYolo(enabled);
        this.stateValue.yoloEnabled = enabled;
      } catch {
        this.stateValue.yoloEnabled = false;
      }
      this.emitChange();
    }
    async toggleAutoModel(enabled) {
      try {
        await this.api.setAutoModel(enabled);
        this.stateValue.autoModelEnabled = enabled;
        if (!enabled) {
          this.stateValue.autoRoute = null;
        }
      } catch {
        this.stateValue.autoModelEnabled = false;
      }
      this.emitChange();
    }
    async cycleReasoningEffort() {
      const order = [null, "off", "high", "max"];
      const current = this.stateValue.reasoningEffort;
      const idx = order.indexOf(current);
      const next = order[(idx + 1) % order.length];
      try {
        await this.api.setReasoningEffort(next);
        this.stateValue.reasoningEffort = next;
      } catch {
      }
      this.emitChange();
    }
    async setReasoningEffort(effort) {
      try {
        await this.api.setReasoningEffort(effort);
        this.stateValue.reasoningEffort = effort;
      } catch {
      }
      this.emitChange();
    }
    async toggleLoop(enabled) {
      if (enabled && !this.stateValue.loopAvailable) {
        this.notifications.notify("Task is not suitable for loop execution");
        this.stateValue.loopEnabled = false;
        this.emitChange();
        return;
      }
      this.stateValue.loopEnabled = enabled;
      await this.api.setLoop(enabled).catch(() => {
      });
      this.emitChange();
    }
    async checkLoopSuitability(prompt) {
      const suitable = this.isLoopSuitable(prompt);
      const changed = suitable !== this.stateValue.loopAvailable;
      this.stateValue.loopAvailable = suitable;
      if (!suitable && this.stateValue.loopEnabled) {
        this.stateValue.loopEnabled = false;
        await this.api.setLoop(false).catch(() => {
        });
      }
      if (changed) this.emitChange();
    }
    isLoopSuitable(prompt) {
      const trimmed = prompt.trim();
      if (trimmed.length < 10) return false;
      const lower = trimmed.toLowerCase();
      const loopKeywords = [
        // Chinese
        "\u5FAA\u73AF",
        "\u53CD\u590D",
        "\u91CD\u590D",
        "\u4E0D\u65AD",
        "\u6301\u7EED",
        "\u6BCF\u6B21",
        "\u6BCF\u4E2A",
        "\u6BCF\u4E00\u4E2A",
        "\u904D\u5386",
        "\u8FED\u4EE3",
        "\u4E00\u76F4",
        "\u76F4\u5230",
        "\u4E3A\u6B62",
        "\u6279\u91CF",
        "\u6240\u6709\u6587\u4EF6",
        "\u5168\u90E8\u6587\u4EF6",
        "\u6BCF\u4E2A\u6587\u4EF6",
        "\u6BCF\u9694",
        "\u5B9A\u65F6",
        "\u76D1\u63A7",
        "\u76D1\u542C",
        "\u5B9E\u65F6",
        // English
        "loop",
        "iterate",
        "repeatedly",
        "until",
        "keep doing",
        "keep running",
        "for each",
        "for every",
        "all files",
        "every file",
        "batch",
        "continuously",
        "monitor",
        "watch for",
        "periodically",
        "in a loop",
        "retry",
        "repeat",
        "cycle through",
        "poll"
      ];
      return loopKeywords.some((kw) => lower.includes(kw));
    }
    async saveWorkflow(nodes) {
      try {
        await this.api.saveWorkflow(nodes);
        await this.loadWorkflowState();
        this.emitChange();
        this.notifications.notify(nodes.length ? "Workflow saved" : "Workflow cleared");
      } catch (error) {
        this.notifyError(error, "Failed to save workflow");
      }
    }
    async resetWorkflow() {
      try {
        await this.api.resetWorkflow();
        await this.loadWorkflowState();
        this.emitChange();
        this.notifications.notify("Workflow reset");
      } catch (error) {
        this.notifyError(error, "Failed to reset workflow");
      }
    }
    async sendPrompt(rawPrompt) {
      const prompt = rawPrompt.trim();
      if (!prompt || this.stateValue.isRunning) {
        return;
      }
      this.stateValue.messages.push({ role: "user", content: prompt });
      this.ensureChatTab();
      this.stateValue.activeTabId = "chat";
      this.stateValue.inputValue = "";
      this.stateValue.isRunning = true;
      this.emitChange();
      try {
        const payload = await this.api.chat(prompt);
        if (payload.handled) {
          this.stateValue.messages = payload.messages || this.stateValue.messages;
          this.stateValue.isRunning = false;
          await this.syncSessionState();
          this.emitChange();
          if (payload.notice) {
            this.notifications.notify(payload.notice);
          }
          return;
        }
        if (!payload.task_id) {
          throw new Error(payload.error || "Task creation failed");
        }
        this.stateValue.pendingTaskId = payload.task_id;
        this.stateValue.messages.push({ role: "assistant", content: "...", streaming: true });
        this.stateValue.taskPlaceholderIndex = this.stateValue.messages.length - 1;
        this.emitChange();
        await this.pollTask(payload.task_id);
      } catch (error) {
        this.stateValue.isRunning = false;
        this.emitChange();
        this.notifyError(error, "Failed to send prompt");
      }
    }
    async stopTask() {
      try {
        await this.api.stop();
      } catch {
      }
      this.stateValue.isRunning = false;
      this.stateValue.pendingTaskId = null;
      this.stateValue.taskPlaceholderIndex = null;
      this.emitChange();
    }
    async restoreSession(index, checkpoint) {
      try {
        const payload = await this.api.restoreSession(index, checkpoint);
        this.stateValue.messages = payload.messages || [];
        this.stateValue.activeTabId = "chat";
        this.ensureChatTab();
        await this.syncSessionState();
        this.emitChange();
        this.notifications.notify(
          checkpoint ? `Restored session #${index} @ checkpoint ${checkpoint}` : `Restored session #${index}`
        );
      } catch (error) {
        this.notifyError(error, "Failed to restore session");
      }
    }
    async forkSession(index, checkpoint) {
      try {
        const payload = await this.api.forkSession(index, checkpoint);
        this.stateValue.messages = payload.messages || [];
        this.stateValue.activeTabId = "chat";
        this.ensureChatTab();
        await this.syncSessionState();
        this.emitChange();
        this.notifications.notify(
          checkpoint ? `Forked session #${index} @ checkpoint ${checkpoint}` : `Forked session #${index}`
        );
      } catch (error) {
        this.notifyError(error, "Failed to fork session");
      }
    }
    async deleteSession(index) {
      try {
        const inspectedDeleted = this.stateValue.checkpointPanelSessionIndex === index;
        const payload = await this.api.deleteSession(index);
        await this.syncSessionState();
        if (inspectedDeleted) {
          this.stateValue.checkpointPanelSessionIndex = this.stateValue.currentSessionIndex;
          this.stateValue.checkpointPanelEntries = this.stateValue.currentSessionCheckpoints;
        }
        this.emitChange();
        this.notifications.notify(
          payload.was_active ? `Deleted current session #${index}` : `Deleted session #${index}`
        );
      } catch (error) {
        this.notifyError(error, "Failed to delete session");
      }
    }
    async openPreviewTab(filePath) {
      try {
        const preview = await this.api.workspacePreview(filePath);
        const existing = this.stateValue.tabs.find(
          (tab) => tab.kind === "preview" && tab.path === preview.path
        );
        const nextTab = existing || {
          id: `preview:${preview.path}`,
          title: preview.rel || preview.name,
          kind: "preview",
          path: preview.path,
          preview
        };
        if (!existing) {
          this.stateValue.tabs.push(nextTab);
        } else {
          existing.preview = preview;
        }
        this.stateValue.activeTabId = nextTab.id;
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to open preview");
      }
    }
    async openDiffTab(filePath) {
      try {
        const payload = await this.api.diff(filePath);
        const diffText = payload.has_changes ? payload.diff : "No changes.";
        const existing = this.stateValue.tabs.find(
          (tab) => tab.kind === "diff" && tab.path === filePath
        );
        const nextTab = existing || {
          id: `diff:${filePath}`,
          title: `Diff \xB7 ${filePath.split(/[\\/]/).pop() || filePath}`,
          kind: "diff",
          path: filePath,
          diff: diffText
        };
        if (!existing) {
          this.stateValue.tabs.push(nextTab);
        } else {
          existing.diff = diffText;
        }
        this.stateValue.activeTabId = nextTab.id;
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to show diff");
      }
    }
    closeTab(id) {
      this.stateValue.tabs = this.stateValue.tabs.filter((tab) => tab.id !== id);
      if (this.stateValue.activeTabId === id) {
        this.stateValue.activeTabId = "chat";
      }
      this.emitChange();
    }
    async revertFile(filePath) {
      try {
        await this.api.revert(filePath);
        await this.loadChanges();
        this.emitChange();
        this.notifications.notify(`Reverted ${filePath}`);
      } catch (error) {
        this.notifyError(error, "Failed to revert file");
      }
    }
    getEditorTheme() {
      return this.stateValue.theme === "daybreak" || this.stateValue.theme === "paperink" ? "vs" : "vs-dark";
    }
    applyTheme(persist = true) {
      document.documentElement.dataset.theme = this.stateValue.theme;
      if (persist) {
        window.localStorage.setItem("generic-coder-theme", this.stateValue.theme);
      }
      this.emitChange();
    }
    setTheme(theme) {
      this.stateValue.theme = theme;
      this.applyTheme();
      void this.api.setTheme(theme);
    }
    async hydrateWorkspacePickerToken() {
      const cached = window.sessionStorage.getItem("generic-coder-picker-token");
      if (cached) {
        this.stateValue.workspacePickerToken = cached;
        return;
      }
      try {
        this.stateValue.workspacePickerToken = await this.api.pickerToken();
        if (this.stateValue.workspacePickerToken) {
          window.sessionStorage.setItem("generic-coder-picker-token", this.stateValue.workspacePickerToken);
        }
      } catch {
        this.stateValue.workspacePickerToken = "";
      }
    }
    async bootstrap() {
      try {
        const data = await this.api.bootstrap();
        this.applyBootstrap(data);
        this.maybeResumePendingTask(data);
        await Promise.all([
          this.loadWorkspaceTree(),
          this.loadSessions(),
          this.loadChanges(),
          this.loadSkills(),
          this.loadWorkflowState(),
          this.refreshPlanStatus()
        ]);
        this.emitChange();
      } catch (error) {
        this.notifyError(error, "Failed to bootstrap workbench");
      }
    }
    applyBootstrap(data) {
      const previousCurrentSessionIndex = this.stateValue.currentSessionIndex;
      this.stateValue.messages = data.messages || [];
      this.stateValue.isRunning = Boolean(data.is_running);
      this.stateValue.pendingTaskId = data.pending_task?.task_id || null;
      this.stateValue.taskPlaceholderIndex = null;
      if (data.is_running && data.pending_task?.task_id) {
        this.ensureTaskPlaceholder(data.pending_task.preview || "Starting task...");
      }
      this.applyModelSettings(data);
      this.stateValue.providerProfiles = data.provider_profiles || this.stateValue.providerProfiles;
      this.stateValue.workspace = data.workspace || this.stateValue.workspace;
      this.syncWorkspaceDraftFromActive();
      this.stateValue.remote = data.remote || this.stateValue.remote;
      this.stateValue.currentMode = data.mode || "work";
      this.stateValue.workflowNodes = data.workflow?.nodes || [];
      this.stateValue.workflowActive = Boolean(data.workflow?.active);
      this.stateValue.workflowCurrentNode = data.workflow?.current_node || 0;
      this.stateValue.multiAgentEnabled = Boolean(data.multi_agent_enabled);
      this.stateValue.oneShotEnabled = Boolean(data.one_shot_enabled);
      this.stateValue.loopEnabled = Boolean(data.loop_enabled);
      this.stateValue.workflowFollowEnabled = Boolean(data.workflow_follow_enabled);
      this.stateValue.computerUseEnabled = Boolean(data.computer_use_enabled);
      this.stateValue.computerUseAvailable = Boolean(data.computer_use_available);
      this.stateValue.yoloEnabled = Boolean(data.yolo_enabled);
      const effort = data.reasoning_effort;
      this.stateValue.reasoningEffort = effort === "off" || effort === "high" || effort === "max" ? effort : null;
      this.stateValue.autoModelEnabled = Boolean(data.auto_model_enabled);
      this.stateValue.autoRoute = data.auto_route || null;
      this.stateValue.currentSessionIndex = data.current_session?.index ?? null;
      this.stateValue.currentSessionActiveCheckpoint = data.current_session?.active_checkpoint ?? null;
      this.stateValue.currentSessionCheckpoints = data.current_session?.checkpoints || [];
      this.stateValue.sessionUsage = data.current_session?.usage_totals || null;
      this.stateValue.lastUsage = data.current_session?.last_usage || null;
      if (this.stateValue.currentSessionIndex === null || this.stateValue.checkpointPanelSessionIndex === null || this.stateValue.checkpointPanelSessionIndex === previousCurrentSessionIndex || this.stateValue.checkpointPanelSessionIndex === this.stateValue.currentSessionIndex) {
        this.stateValue.checkpointPanelSessionIndex = this.stateValue.currentSessionIndex;
        this.stateValue.checkpointPanelEntries = this.stateValue.currentSessionCheckpoints;
      }
      if (data.picker_token && !this.stateValue.workspacePickerToken) {
        this.stateValue.workspacePickerToken = data.picker_token;
        window.sessionStorage.setItem("generic-coder-picker-token", data.picker_token);
      }
      if (data.theme) {
        this.stateValue.theme = data.theme;
        document.documentElement.dataset.theme = data.theme;
      }
      this.ensureChatTab();
    }
    applyModelSettings(data) {
      const models = Array.isArray(data.models?.models) ? data.models.models : [];
      const currentIndex = Number.isInteger(data.model_index) ? Number(data.model_index) : Number.isInteger(data.models?.current_index) ? Number(data.models?.current_index) : 0;
      const currentModel = models[currentIndex];
      this.stateValue.currentModelIndex = currentIndex;
      this.stateValue.models = models;
      this.stateValue.modelLabel = data.model || currentModel?.label || currentModel?.model || "Model offline";
      this.stateValue.llmForm = { ...DEFAULT_LLM_FORM, ...data.llm_form || this.stateValue.llmForm };
    }
    syncWorkspaceDraftFromActive() {
      this.stateValue.workspaceDraftName = this.stateValue.workspace.active?.name || "";
      this.stateValue.workspaceDraftPath = this.stateValue.workspace.active?.path || "";
    }
    async refreshLightweightState() {
      if (this.stateValue.isRunning && this.stateValue.pendingTaskId) {
        return;
      }
      if (this.stateValue.isRunning) {
        const bootstrap = await this.api.bootstrap().catch(() => ({}));
        this.applyBootstrap(bootstrap);
        this.maybeResumePendingTask(bootstrap);
        this.emitChange();
        return;
      }
      await Promise.allSettled([this.loadChanges(), this.loadSessions(), this.refreshPlanStatus()]);
      this.emitChange();
    }
    async loadWorkspaceTree() {
      this.stateValue.workspaceTree = await this.api.workspaceTree().catch(() => []);
      const directoryPaths = new Set(
        this.stateValue.workspaceTree.filter((entry) => entry.type === "dir").map((entry) => entry.path)
      );
      this.stateValue.workspaceCollapsedPaths = this.stateValue.workspaceCollapsedPaths.filter(
        (path) => directoryPaths.has(path)
      );
    }
    async loadSessions() {
      this.stateValue.sessions = await this.api.sessions().catch(() => []);
    }
    async syncSessionState() {
      const bootstrap = await this.api.bootstrap().catch(() => ({}));
      this.applyBootstrap(bootstrap);
      await this.loadSessions();
    }
    async loadChanges() {
      this.stateValue.changes = await this.api.changes().catch(() => []);
    }
    async loadSkills() {
      this.stateValue.skills = await this.api.skills().catch(() => []);
    }
    async loadWorkflowState() {
      try {
        const workflow = await this.api.workflow();
        this.stateValue.workflowNodes = Array.isArray(workflow.nodes) ? workflow.nodes : [];
        this.stateValue.workflowActive = Boolean(workflow.active);
        this.stateValue.workflowCurrentNode = workflow.current_node || 0;
        this.stateValue.currentMode = await this.api.mode();
      } catch {
      }
    }
    async refreshPlanStatus() {
      try {
        const status = await this.api.planStatus();
        this.stateValue.planRemaining = status.remaining;
      } catch {
        this.stateValue.planRemaining = -1;
      }
    }
    async pollTask(taskId) {
      if (this.pollingTaskId === taskId) {
        return;
      }
      this.pollingTaskId = taskId;
      try {
        while (true) {
          const payload = await this.api.task(taskId);
          if (typeof this.stateValue.taskPlaceholderIndex === "number") {
            this.stateValue.messages[this.stateValue.taskPlaceholderIndex] = {
              role: "assistant",
              content: payload.done ? payload.final || payload.preview || "Done" : payload.preview || "...",
              streaming: !payload.done
            };
          }
          this.emitChange();
          if (payload.done) {
            if (payload.usage && typeof payload.usage.prompt_tokens === "number") {
              this.stateValue.lastUsage = {
                prompt_tokens: payload.usage.prompt_tokens || 0,
                completion_tokens: payload.usage.completion_tokens || 0,
                cached_tokens: payload.usage.prompt_cache_hit_tokens ?? 0
              };
            }
            break;
          }
          await new Promise((resolve) => window.setTimeout(resolve, 700));
        }
        this.stateValue.pendingTaskId = null;
        this.stateValue.taskPlaceholderIndex = null;
        this.stateValue.isRunning = false;
        if (this.pollingTaskId === taskId) {
          this.pollingTaskId = null;
        }
        this.applyBootstrap(await this.api.bootstrap().catch(() => ({})));
        await Promise.all([this.loadChanges(), this.loadSessions(), this.refreshPlanStatus()]);
        this.emitChange();
      } catch (error) {
        this.stateValue.pendingTaskId = null;
        this.stateValue.taskPlaceholderIndex = null;
        this.stateValue.isRunning = false;
        if (this.pollingTaskId === taskId) {
          this.pollingTaskId = null;
        }
        this.emitChange();
        this.notifyError(error, "Failed to poll task");
      }
    }
    ensureChatTab() {
      if (!this.stateValue.tabs.find((tab) => tab.id === "chat")) {
        this.stateValue.tabs.unshift({ id: "chat", title: "Chat", kind: "chat" });
      }
    }
    notifyError(error, fallback) {
      this.notifications.notify(error instanceof Error ? error.message : fallback);
    }
    emitChange() {
      this.changeEmitter.fire(this.state);
    }
  };

  // workbench/src/main.ts
  var root = document.getElementById("app");
  if (!root) {
    throw new Error("Workbench root element is missing");
  }
  var services = new ServiceCollection();
  var instantiation = new InstantiationService(services);
  var disposables = new DisposableStore();
  var layoutService = services.set(ILayoutService, new LayoutService(root));
  layoutService.renderShell();
  void window.electronAPI?.getPlatform?.().then((platform) => {
    document.documentElement.dataset.platform = platform;
  }).catch((error) => {
    console.warn("Failed to detect platform for titlebar layout", error);
  });
  var commandService = services.set(ICommandService, new CommandService(instantiation));
  var notificationService = services.set(INotificationService, instantiation.createInstance(NotificationService));
  var workbenchService = services.set(IWorkbenchService, instantiation.createInstance(WorkbenchService));
  commandService.registerCoreCommands();
  disposables.add(instantiation.createInstance(ActivitybarPart));
  disposables.add(instantiation.createInstance(SidebarPart));
  disposables.add(instantiation.createInstance(EditorPart));
  disposables.add(instantiation.createInstance(ComposerPart));
  disposables.add(instantiation.createInstance(StatusbarPart));
  disposables.add(instantiation.createInstance(QuickOpenPart));
  layoutService.getRoot().addEventListener("click", (event) => {
    const command = event.target.closest("[data-command]")?.dataset.command;
    if (!command) {
      return;
    }
    void commandService.executeCommand(command);
  });
  void workbenchService.start();
  window.addEventListener("beforeunload", () => {
    disposables.dispose();
  });
})();
