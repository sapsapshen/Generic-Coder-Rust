export class LayoutService {
  constructor(private readonly root: HTMLElement) {}

  renderShell(): void {
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
              <div class="composer__body">
                <textarea id="prompt-input" spellcheck="false" placeholder="Describe the task, or type /new to start a clean session."></textarea>
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

  getElement<T extends HTMLElement>(id: string): T {
    const element = document.getElementById(id);
    if (!element) {
      throw new Error(`Missing layout element: ${id}`);
    }
    return element as T;
  }

  getRoot(): HTMLElement {
    return this.root;
  }
}
