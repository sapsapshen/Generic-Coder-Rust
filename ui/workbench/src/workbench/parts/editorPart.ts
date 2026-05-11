import { Disposable } from '../../base/common/lifecycle';
import { loadMonaco } from '../../monaco';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { inferLanguage, escapeHtml } from '../common/dom';
import type { EditorTab } from '../common/state';
import { LayoutService } from '../services/layoutService';
import { ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

/** Parse message content splitting out <thinking>...</thinking> blocks. */
function renderMessageContent(raw: string, showAgentLogs: boolean, streaming?: boolean): string {
  const sanitized = raw
    .replace(/<(?:tool_use|tool_call)>[\s\S]*?(?:<\/(?:tool_use|tool_call)>|$)/g, '')
    .replace(/<summary>[\s\S]*?(?:<\/summary>|$)/g, '')
    .replace(/<\/?(?:summary|tool_use|tool_call)>/g, '')
    .trim();
  const thinkingRe = /<thinking>([\s\S]*?)<\/thinking>/g;
  const parts: string[] = [];
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = thinkingRe.exec(sanitized)) !== null) {
    if (m.index > last) {
      parts.push(`<pre class="message__text">${escapeHtml(sanitized.slice(last, m.index))}</pre>`);
    }
    if (showAgentLogs) {
      parts.push(
        `<details class="thinking-block" open>` +
        `<summary class="thinking-block__summary"><i class="codicon codicon-lightbulb"></i> Reasoning</summary>` +
        `<pre class="thinking-block__content">${escapeHtml(m[1].trim())}</pre>` +
        `</details>`,
      );
    }
    last = m.index + m[0].length;
  }
  const tail = sanitized.slice(last);
  if (tail) {
    parts.push(`<pre class="message__text${streaming ? ' message__text--streaming' : ''}">${escapeHtml(tail)}</pre>`);
  }
  return parts.join('') || `<pre class="message__text${streaming ? ' message__text--streaming' : ''}"></pre>`;
}

export class EditorPart extends Disposable {
  private editorInstance: import('monaco-editor').editor.IStandaloneCodeEditor | null = null;
  private editorModel: import('monaco-editor').editor.ITextModel | null = null;
  private renderKey = '';

  private readonly layoutService: LayoutService;
  private readonly workbenchService: WorkbenchService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.layoutService = accessor.get(ILayoutService);
    this.workbenchService = accessor.get(IWorkbenchService);
    this._register(this.workbenchService.onDidChangeState(() => this.render()));
    this.render();
  }

  render(): void {
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
    const surface = this.layoutService.getElement<HTMLDivElement>('editor-surface');

    if (activeTab.kind === 'chat') {
      this.disposeEditor();
      surface.innerHTML = `
        <div class="chat-feed" id="chat-feed">
          ${this.workbenchService.state.messages
            .map((message) => `
                <article class="message message--${escapeHtml(message.role)}">
                  <div class="message__avatar"><i class="codicon codicon-${message.role === 'user' ? 'account' : 'sparkle'}"></i></div>
                  <div class="message__body">
                    <div class="message__role">${escapeHtml(message.role)}</div>
                    <div class="message__content">${renderMessageContent(message.content || '', this.workbenchService.state.showAgentLogs, message.streaming)}</div>
                  </div>
                </article>`,
            )
            .join('')}
        </div>`;
      const feed = surface.querySelector<HTMLDivElement>('#chat-feed');
      if (feed) {
        feed.scrollTop = feed.scrollHeight;
      }
      return;
    }

    if (activeTab.kind === 'preview') {
      const preview = activeTab.preview;
      surface.innerHTML = `
        <div class="editor-header">
          <div>
            <strong>${escapeHtml(preview.rel || preview.name)}</strong>
            <div class="muted">${escapeHtml(preview.mime)} · ${preview.size} bytes${preview.truncated ? ' · truncated' : ''}</div>
          </div>
          <div class="editor-header__actions">
            <button class="text-button" data-copy-path="${escapeHtml(preview.rel || preview.path)}">Mention</button>
          </div>
        </div>
        <div class="preview-host" id="preview-host"></div>`;
      surface.querySelector('[data-copy-path]')?.addEventListener('click', () => {
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
    void this.renderTextEditor(activeTab.diff, 'diff');
  }

  private renderTabs(): void {
    const tabs = this.layoutService.getElement<HTMLDivElement>('editor-tabs');
    tabs.innerHTML = this.workbenchService.state.tabs
      .map((tab) => {
        const closable = tab.id !== 'chat';
        return `
          <button class="tab${tab.id === this.workbenchService.state.activeTabId ? ' is-active' : ''}" data-tab-id="${tab.id}">
            <span>${escapeHtml(tab.title)}</span>
            ${closable ? `<i class="codicon codicon-close" data-close-tab="${tab.id}"></i>` : ''}
          </button>`;
      })
      .join('');
    tabs.querySelectorAll<HTMLElement>('[data-tab-id]').forEach((button) => {
      button.addEventListener('click', (event) => {
        const closeTarget = (event.target as HTMLElement).closest('[data-close-tab]');
        if (closeTarget) {
          const id = closeTarget.getAttribute('data-close-tab');
          if (id) {
            this.workbenchService.closeTab(id);
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

  private async renderPreview(preview: Extract<EditorTab, { kind: 'preview' }>['preview']): Promise<void> {
    const host = this.layoutService.getElement<HTMLDivElement>('preview-host');
    if (preview.kind === 'image') {
      this.disposeEditor();
      host.innerHTML = `<div class="image-preview"><img src="/api/workspace/preview-content?path=${encodeURIComponent(preview.path)}" alt="${escapeHtml(preview.name)}" /></div>`;
      return;
    }
    if (preview.kind === 'binary') {
      await this.renderTextEditor(preview.message || 'Binary preview is not supported.', 'plaintext');
      return;
    }
    await this.renderTextEditor(preview.content || '', inferLanguage(preview.path));
  }

  private async renderTextEditor(value: string, language: string): Promise<void> {
    const host = this.layoutService.getElement<HTMLDivElement>('preview-host');
    host.innerHTML = '';
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
      theme: this.workbenchService.getEditorTheme(),
    });
  }

  private computeRenderKey(activeTab: EditorTab): string {
    if (activeTab.kind === 'chat') {
      const lastMessage = this.workbenchService.state.messages[this.workbenchService.state.messages.length - 1];
      return `chat:${this.workbenchService.state.activeTabId}:${this.workbenchService.state.showAgentLogs}:${this.workbenchService.state.messages.length}:${lastMessage?.content || ''}`;
    }
    if (activeTab.kind === 'preview') {
      return `preview:${activeTab.path}:${activeTab.preview.kind}:${activeTab.preview.size}:${this.workbenchService.state.theme}`;
    }
    return `diff:${activeTab.path}:${activeTab.diff.length}:${this.workbenchService.state.theme}`;
  }

  private disposeEditor(): void {
    this.editorInstance?.dispose();
    this.editorInstance = null;
    this.editorModel?.dispose();
    this.editorModel = null;
  }
}
