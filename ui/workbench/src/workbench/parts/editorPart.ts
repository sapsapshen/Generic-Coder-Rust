import { Disposable } from '../../base/common/lifecycle';
import { loadMonaco } from '../../monaco';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { inferLanguage, escapeHtml } from '../common/dom';
import type { EditorTab } from '../common/state';
import { LayoutService } from '../services/layoutService';
import { ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

const MODE_DISPLAY: Record<string, { icon: string; label: string }> = {
  ask:    { icon: 'comment-discussion', label: 'Ask' },
  plan:   { icon: 'list-ordered',       label: 'Plan' },
  build:  { icon: 'tools',              label: 'Build' },
  review: { icon: 'eye',                label: 'Review' },
  work:   { icon: 'tools',              label: 'Work' },
};

function parseToolBlock(rawBlock: string): { name: string; args: string } | null {
  try {
    const obj = JSON.parse(rawBlock.trim());
    const name = obj.name || obj.tool || '(tool)';
    const input = obj.input || obj.arguments || obj.parameters || obj.params || {};
    const args = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
    return { name, args };
  } catch {
    return null;
  }
}

function renderDetailEvents(detailEvents: any[]): string {
  if (!detailEvents || detailEvents.length === 0) return '';
  const items = detailEvents.map((ev: any) => {
    const type = ev.type || ev.event || '';
    let emoji = '▸';
    let text = '';
    if (type === 'acp_plan' || type === 'oneshot_plan') {
      emoji = '📋'; text = escapeHtml(ev.plan || ev.content || 'Plan ready');
    } else if (type === 'acp_step_start') {
      emoji = '▶'; text = escapeHtml(ev.label || ev.step || `Step ${(ev.index ?? 0) + 1}`);
    } else if (type === 'acp_step_done' || type === 'oneshot_done') {
      emoji = '✅'; text = escapeHtml(ev.label || ev.summary || 'Step done');
    } else if (type === 'acp_step_failed') {
      emoji = '❌'; text = escapeHtml(ev.label || ev.error || 'Step failed');
    } else if (type === 'acp_done') {
      emoji = '🏁'; text = escapeHtml(ev.summary || 'Done');
    } else {
      text = escapeHtml(type + (ev.label ? ': ' + ev.label : ''));
    }
    return `<div class="event-block__item">${emoji} ${text}</div>`;
  });
  return `<div class="event-block">${items.join('')}</div>`;
}

function renderModeBadge(mode?: string): string {
  if (!mode) return '';
  const info = MODE_DISPLAY[mode];
  if (!info) return '';
  return `<span class="mode-badge mode-badge--${escapeHtml(mode)}"><i class="codicon codicon-${info.icon}"></i>${info.label}</span>`;
}

function renderAgentWorkingIndicator(): string {
  return `<div class="agent-working"><div class="agent-working__dot"></div><div class="agent-working__dot"></div><div class="agent-working__dot"></div><span class="agent-working__label">Agent is working…</span></div>`;
}

function renderMessageContent(message: any, showDetailedAgentLogs: boolean): string {
  const streaming = message.streaming;
  const raw: string = message.content || '';

  // For agent-log messages
  if (message.kind === 'agent-log') {
    if (!showDetailedAgentLogs) {
      if (streaming) {
        return renderAgentWorkingIndicator();
      }
      return ''; // filtered out in buildChatFeedHtml
    }
    // Detailed view: render events + tool blocks
    let html = '';
    if (message.detail_events?.length) {
      html += renderDetailEvents(message.detail_events);
    }
    // Render tool use blocks from content
    const toolRe = /<(?:tool_use|tool_call)>([\s\S]*?)<\/(?:tool_use|tool_call)>/g;
    let toolMatch: RegExpExecArray | null;
    while ((toolMatch = toolRe.exec(raw)) !== null) {
      const parsed = parseToolBlock(toolMatch[1]);
      if (parsed) {
        html += `<details class="tool-block"><summary class="tool-block__summary"><i class="codicon codicon-tools"></i> ${escapeHtml(parsed.name)}</summary><pre class="tool-block__content">${escapeHtml(parsed.args)}</pre></details>`;
      }
    }
    // Render main text (stripped of tool blocks)
    const stripped = raw
      .replace(/<(?:tool_use|tool_call)>[\s\S]*?(?:<\/(?:tool_use|tool_call)>|$)/g, '')
      .replace(/<summary>[\s\S]*?(?:<\/summary>|$)/g, '')
      .replace(/<\/?(?:summary|tool_use|tool_call)>/g, '')
      .trim();
    if (stripped) {
      html += `<pre class="message__text${streaming ? ' message__text--streaming' : ''}">${escapeHtml(stripped)}</pre>`;
    }
    return html || renderAgentWorkingIndicator();
  }

  // Regular assistant/user messages
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
    if (showDetailedAgentLogs) {
      const isStreamingThinking = streaming && m.index + m[0].length >= sanitized.length;
      parts.push(
        `<details class="thinking-block" open>` +
        `<summary class="thinking-block__summary"><i class="codicon codicon-lightbulb"></i> Thinking${isStreamingThinking ? '<span class="thinking-block__streaming-dot"></span>' : ''}</summary>` +
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

function buildMessageHtml(message: any, showDetailedAgentLogs: boolean): string {
  const isAgentLog = message.kind === 'agent-log';

  // Filter out non-streaming finished agent-log messages when detailed logs off
  if (isAgentLog && !showDetailedAgentLogs && !message.streaming) {
    return '';
  }

  const roleClass = isAgentLog ? 'agent' : message.role === 'user' ? 'user' : 'assistant';
  const avatarIcon = isAgentLog ? 'terminal' : message.role === 'user' ? 'account' : 'sparkle';
  const avatarClass = isAgentLog ? 'message__avatar--agent' : message.role === 'assistant' ? 'message__avatar--assistant' : '';

  const roleLabel = isAgentLog
    ? (message.streaming ? 'agent · working' : 'agent log')
    : message.role;

  const spinner = message.streaming
    ? '<span class="message__spinner"></span>'
    : '';

  const modeBadge = !isAgentLog && message.mode ? renderModeBadge(message.mode) : '';

  const contentHtml = renderMessageContent(message, showDetailedAgentLogs);

  return `
    <article class="message message--${escapeHtml(roleClass)}${message.streaming ? ' message--streaming' : ''}">
      <div class="message__avatar ${avatarClass}"><i class="codicon codicon-${avatarIcon}"></i></div>
      <div class="message__body">
        <div class="message__meta">
          <span class="message__role">${escapeHtml(roleLabel)}</span>
          ${spinner}
          ${modeBadge}
        </div>
        <div class="message__content">${contentHtml}</div>
      </div>
    </article>`;
}

function buildChatFeedHtml(messages: any[], showDetailedAgentLogs: boolean): string {
  if (messages.length === 0) {
    return `
      <div class="chat-empty">
        <div class="chat-empty__icon"><i class="codicon codicon-sparkle"></i></div>
        <div class="chat-empty__text">Start a conversation — describe a task or ask a question.</div>
        <div class="chat-empty__modes">
          ${renderModeBadge('ask')}
          ${renderModeBadge('plan')}
          ${renderModeBadge('build')}
          ${renderModeBadge('review')}
        </div>
      </div>`;
  }
  return messages.map((m) => buildMessageHtml(m, showDetailedAgentLogs)).join('');
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
      this.renderChat(surface);
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

  private renderChat(surface: HTMLDivElement): void {
    const state = this.workbenchService.state;
    const feed = surface.querySelector<HTMLDivElement>('#chat-feed');
    const wasAtBottom = feed
      ? feed.scrollTop + feed.clientHeight >= feed.scrollHeight - 40
      : true;

    surface.innerHTML = `<div class="chat-feed" id="chat-feed">${buildChatFeedHtml(state.messages, state.showDetailedAgentLogs)}</div>`;

    const newFeed = surface.querySelector<HTMLDivElement>('#chat-feed');
    if (newFeed && wasAtBottom) {
      newFeed.scrollTop = newFeed.scrollHeight;
    }
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
      const state = this.workbenchService.state;
      const lastMessage = state.messages[state.messages.length - 1];
      return `chat:${state.activeTabId}:${state.showDetailedAgentLogs}:${state.messages.length}:${lastMessage?.content || ''}:${lastMessage?.streaming}`;
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
