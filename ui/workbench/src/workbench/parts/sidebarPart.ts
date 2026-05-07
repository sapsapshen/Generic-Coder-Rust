import { Disposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import type { WorkspaceEntry } from '../../types';
import { escapeHtml } from '../common/dom';
import { DEFAULT_REMOTE_FORM, THEME_OPTIONS, type WorkbenchState } from '../common/state';
import { LayoutService } from '../services/layoutService';
import { ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

const MODEL_PRICING: Record<string, { cacheHit: number; cacheMiss: number; output: number }> = {
  'deepseek-v4-pro': { cacheHit: 0.003625, cacheMiss: 0.435, output: 0.87 },
  'deepseek-v4-flash': { cacheHit: 0.0028, cacheMiss: 0.14, output: 0.28 },
  'deepseek-reasoner': { cacheHit: 0.003625, cacheMiss: 0.435, output: 0.87 },
  'deepseek-chat': { cacheHit: 0.0028, cacheMiss: 0.14, output: 0.28 },
};

function estimateCost(model: string, promptTokens: number, completionTokens: number, cachedTokens: number): string {
  const pricing = Object.entries(MODEL_PRICING).find(([key]) => model.toLowerCase().includes(key))?.[1];
  if (!pricing) {
    return 'n/a';
  }
  const missTokens = Math.max(promptTokens - cachedTokens, 0);
  const cost =
    (cachedTokens / 1_000_000) * pricing.cacheHit +
    (missTokens / 1_000_000) * pricing.cacheMiss +
    (completionTokens / 1_000_000) * pricing.output;
  return cost < 0.001 ? '<$0.001' : `$${cost.toFixed(4)}`;
}

function hitRate(promptTokens = 0, cachedTokens = 0): string {
  if (!promptTokens) {
    return '0%';
  }
  return `${Math.round((cachedTokens / promptTokens) * 100)}%`;
}

function normalizeProviderBase(base: string): string {
  return base.trim().toLowerCase().replace(/\/beta$/, '').replace(/\/v1$/, '');
}

function sanitizePreview(preview: string): string {
  return preview
    .replace(/<thinking>[\s\S]*?(?:<\/thinking>|$)/gi, '')
    .replace(/<summary>[\s\S]*?(?:<\/summary>|$)/gi, '')
    .replace(/<(?:tool_use|tool_call)>[\s\S]*?(?:<\/(?:tool_use|tool_call)>|$)/gi, '')
    .replace(/<\/?(?:thinking|summary|tool_use|tool_call)>/gi, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function matchProviderProfileId(state: Readonly<WorkbenchState>): string {
  const currentBase = (state.llmForm.apibase || '').trim().toLowerCase();
  const currentModel = (state.llmForm.model || '').trim().toLowerCase();
  const normalizedCurrentBase = normalizeProviderBase(currentBase);
  const exactMatch = state.providerProfiles.find(
    (profile) => normalizeProviderBase(profile.apibase) === normalizedCurrentBase && profile.model.trim().toLowerCase() === currentModel,
  );
  const modelMatch = state.providerProfiles.find((profile) => profile.model.trim().toLowerCase() === currentModel);
  const providerMatch = state.providerProfiles.find(
    (profile) => normalizeProviderBase(profile.apibase) === normalizedCurrentBase,
  );
  return exactMatch?.id || modelMatch?.id || providerMatch?.id || state.providerProfiles[0]?.id || '';
}

export class SidebarPart extends Disposable {
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
    const state = this.workbenchService.state;
    const title = this.layoutService.getElement<HTMLSpanElement>('sidebar-title');
    const content = this.layoutService.getElement<HTMLDivElement>('sidebar-content');
    const titles = {
      chat: 'Assistant',
      explorer: 'Explorer',
      scm: 'Source Control',
      extensions: 'Skills',
      settings: 'Settings',
      workflow: 'Workflow',
    } as const;

    title.textContent = titles[state.activeView];
    switch (state.activeView) {
      case 'chat':
        content.innerHTML = this.renderChat();
        this.bindChat(content);
        break;
      case 'explorer':
        content.innerHTML = this.renderExplorer();
        this.bindExplorer(content);
        break;
      case 'scm':
        content.innerHTML = this.renderScm();
        this.bindScm(content);
        break;
      case 'extensions':
        content.innerHTML = this.renderExtensions();
        this.bindExtensions(content);
        break;
      case 'settings':
        content.innerHTML = this.renderSettings();
        this.bindSettings(content);
        break;
      case 'workflow':
        content.innerHTML = this.renderWorkflow();
        this.bindWorkflow(content);
        break;
    }
  }

  private renderChat(): string {
    const state = this.workbenchService.state;
    const effectiveModel = state.autoRoute?.model || state.effectiveModel || state.llmForm.model || '';
    const lastUsage = state.lastUsage;
    const sessionUsage = state.sessionUsage;
    const inferencePanel = `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Inference</span></div>
        <div class="kv-list">
          <div class="kv-row"><span>Route</span><strong>${escapeHtml(state.autoRoute?.display_name || effectiveModel || 'n/a')}</strong></div>
          <div class="kv-row"><span>Last turn</span><strong>${lastUsage ? estimateCost(effectiveModel, lastUsage.prompt_tokens, lastUsage.completion_tokens, lastUsage.cached_tokens || 0) : 'n/a'}</strong></div>
          <div class="kv-row"><span>Last cache hit</span><strong>${lastUsage ? hitRate(lastUsage.prompt_tokens, lastUsage.cached_tokens || 0) : '0%'}</strong></div>
          <div class="kv-row"><span>Session cost</span><strong>${sessionUsage ? estimateCost(effectiveModel, sessionUsage.prompt_tokens, sessionUsage.completion_tokens, sessionUsage.cached_tokens || 0) : 'n/a'}</strong></div>
          <div class="kv-row"><span>Session cache hit</span><strong>${sessionUsage ? hitRate(sessionUsage.prompt_tokens, sessionUsage.cached_tokens || 0) : '0%'}</strong></div>
          <div class="kv-row"><span>Turns</span><strong>${sessionUsage?.turns || 0}</strong></div>
        </div>
      </section>`;

    const checkpointSessionIndex = state.checkpointPanelSessionIndex || state.currentSessionIndex;
    const checkpointRowsSource = state.checkpointPanelEntries.length > 0 || checkpointSessionIndex !== state.currentSessionIndex
      ? state.checkpointPanelEntries
      : state.currentSessionCheckpoints;
    const sessionRows =
      state.sessions.length === 0
        ? '<div class="empty-state">No saved sessions yet.</div>'
        : state.sessions
            .map(
              (session) => `
                <div class="list-row list-row--card${session.current ? ' is-active' : ''}">
                  <div>
                    <strong>#${session.index}</strong>
                    <div class="muted">${escapeHtml(session.relative_time)} · ${session.rounds} rounds · ${session.checkpoint_count || 0} checkpoints</div>
                  </div>
                  <span class="list-row__preview">${escapeHtml(sanitizePreview(session.preview || ''))}</span>
                  <div class="inline-actions">
                    <button class="text-button" data-inspect-session="${session.index}">Points</button>
                    <button class="text-button" data-restore-session="${session.index}">Restore</button>
                    <button class="text-button" data-fork-session="${session.index}">Fork</button>
                    <button class="text-button danger" data-delete-session="${session.index}">Delete</button>
                  </div>
                </div>`,
            )
            .join('');

    const checkpointRows =
      checkpointRowsSource.length === 0
        ? '<div class="empty-state">Select a session or start a task to inspect restore points.</div>'
        : checkpointRowsSource
            .map(
              (checkpoint) => `
                <div class="list-row list-row--card${checkpointSessionIndex === state.currentSessionIndex && checkpoint.index === state.currentSessionActiveCheckpoint ? ' is-active' : ''}">
                  <div>
                    <strong>Checkpoint ${checkpoint.index}</strong>
                    <div class="muted">${escapeHtml(checkpoint.relative_time)} · ${checkpoint.rounds} rounds</div>
                  </div>
                  <span class="list-row__preview">${escapeHtml(sanitizePreview(checkpoint.preview || ''))}</span>
                  <div class="inline-actions">
                    <button class="text-button" data-restore-checkpoint="${checkpoint.index}">Restore</button>
                    <button class="text-button" data-fork-checkpoint="${checkpoint.index}">Fork</button>
                  </div>
                </div>`,
            )
            .join('');
    return `
      ${inferencePanel}
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Sessions</span><button class="text-button" data-new-chat="1">New</button></div>
        <div class="sidebar-list">${sessionRows}</div>
      </section>
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Restore Points</span><span class="muted">${checkpointSessionIndex ? `Session #${checkpointSessionIndex}` : 'No active session'}</span></div>
        <div class="sidebar-list">${checkpointRows}</div>
      </section>`;
  }

  private bindChat(container: HTMLElement): void {
    container.querySelector('[data-new-chat="1"]')?.addEventListener('click', () => {
      void this.workbenchService.sendPrompt('/new');
    });
    container.querySelectorAll<HTMLElement>('[data-inspect-session]').forEach((button) => {
      button.addEventListener('click', () => {
        void this.workbenchService.inspectSessionCheckpoints(Number(button.dataset.inspectSession));
      });
    });
    container.querySelectorAll<HTMLElement>('[data-restore-session]').forEach((button) => {
      button.addEventListener('click', () => {
        void this.workbenchService.restoreSession(Number(button.dataset.restoreSession));
      });
    });
    container.querySelectorAll<HTMLElement>('[data-fork-session]').forEach((button) => {
      button.addEventListener('click', () => {
        void this.workbenchService.forkSession(Number(button.dataset.forkSession));
      });
    });
    container.querySelectorAll<HTMLElement>('[data-delete-session]').forEach((button) => {
      button.addEventListener('click', () => {
        const index = Number(button.dataset.deleteSession);
        const isCurrent = this.workbenchService.state.currentSessionIndex === index;
        const confirmed = window.confirm(
          isCurrent
            ? `Delete current session #${index}? This also clears the current chat view.`
            : `Delete session #${index}?`,
        );
        if (confirmed) {
          void this.workbenchService.deleteSession(index);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-restore-checkpoint]').forEach((button) => {
      button.addEventListener('click', () => {
        const checkpoint = Number(button.dataset.restoreCheckpoint);
        const sessionIndex = this.workbenchService.state.checkpointPanelSessionIndex || this.workbenchService.state.currentSessionIndex;
        if (sessionIndex) {
          void this.workbenchService.restoreSession(sessionIndex, checkpoint);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-fork-checkpoint]').forEach((button) => {
      button.addEventListener('click', () => {
        const checkpoint = Number(button.dataset.forkCheckpoint);
        const sessionIndex = this.workbenchService.state.checkpointPanelSessionIndex || this.workbenchService.state.currentSessionIndex;
        if (sessionIndex) {
          void this.workbenchService.forkSession(sessionIndex, checkpoint);
        }
      });
    });
  }

  private renderExplorer(): string {
    const state = this.workbenchService.state;
    const collapsedPaths = new Set(state.workspaceCollapsedPaths);
    const visibleEntries = this.getVisibleWorkspaceEntries(state.workspaceTree, collapsedPaths);
    const rows =
      visibleEntries.length === 0
        ? '<div class="empty-state">No workspace open.</div>'
        : visibleEntries
            .map((entry) => {
              const isCollapsed = entry.type === 'dir' && collapsedPaths.has(entry.path);
              return `
                <button
                  class="tree-row tree-row--${entry.type}${isCollapsed ? ' is-collapsed' : ''}"
                  data-path="${escapeHtml(entry.path)}"
                  ${entry.type === 'dir' ? 'data-tree-folder="1"' : ''}
                  style="--depth:${entry.depth}"
                  ${entry.type === 'dir' ? `aria-expanded="${isCollapsed ? 'false' : 'true'}"` : ''}
                >
                  <span class="tree-row__twistie${entry.type === 'file' ? ' tree-row__twistie--placeholder' : ''}">
                    ${entry.type === 'dir' ? `<i class="codicon codicon-${isCollapsed ? 'chevron-right' : 'chevron-down'}"></i>` : ''}
                  </span>
                  <i class="codicon codicon-${entry.type === 'dir' ? (isCollapsed ? 'folder' : 'folder-opened') : 'file'}"></i>
                  <span class="tree-row__label">${escapeHtml(entry.name)}</span>
                </button>`;
            })
            .join('');
    return `
      <section class="sidebar-section">
        <div class="sidebar-section__header">
          <span>${escapeHtml(state.workspace.active?.path || 'Workspace')}</span>
          <button class="text-button" data-refresh-tree="1">Refresh</button>
        </div>
        <div class="tree-list">${rows}</div>
      </section>`;
  }

  private bindExplorer(container: HTMLElement): void {
    container.querySelector('[data-refresh-tree="1"]')?.addEventListener('click', () => {
      void this.workbenchService.refreshAll();
    });
    container.querySelectorAll<HTMLElement>('[data-tree-folder="1"]').forEach((button) => {
      button.addEventListener('click', () => {
        const path = button.dataset.path;
        if (path) {
          this.workbenchService.toggleWorkspaceFolder(path);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('.tree-row--file').forEach((button) => {
      button.addEventListener('click', () => {
        const path = button.dataset.path;
        if (path) {
          void this.workbenchService.openPreviewTab(path);
        }
      });
    });
  }

  private getVisibleWorkspaceEntries(entries: readonly WorkspaceEntry[], collapsedPaths: ReadonlySet<string>): WorkspaceEntry[] {
    const visibleEntries: WorkspaceEntry[] = [];
    const collapsedDepths: number[] = [];

    for (const entry of entries) {
      while (collapsedDepths.length > 0 && entry.depth <= collapsedDepths[collapsedDepths.length - 1]) {
        collapsedDepths.pop();
      }

      if (collapsedDepths.length > 0) {
        continue;
      }

      visibleEntries.push(entry);
      if (entry.type === 'dir' && collapsedPaths.has(entry.path)) {
        collapsedDepths.push(entry.depth);
      }
    }

    return visibleEntries;
  }

  // ── Workflow view ─────────────────────────────────────────────────────────

  private renderWorkflow(): string {
    const state = this.workbenchService.state;
    const nodes = state.workflowNodes;
    const active = state.workflowActive;
    const currentNode = state.workflowCurrentNode;

    const MODE_META: Record<string, { icon: string; label: string; description: string; color: string }> = {
      work: { icon: 'tools', label: 'Work', description: 'Implement & execute tasks', color: '#4ec9b0' },
      plan: { icon: 'list-ordered', label: 'Plan', description: 'Explore & design without touching code', color: '#9cdcfe' },
      review: { icon: 'eye', label: 'Review', description: 'Audit code for issues & suggest fixes', color: '#ce9178' },
    };

    const statusBanner = active
      ? `<div class="wf-status wf-status--active">
           <i class="codicon codicon-run-all"></i>
           <span>Workflow running — step ${currentNode + 1} of ${nodes.length}</span>
         </div>`
      : nodes.length
        ? `<div class="wf-status wf-status--idle">
             <i class="codicon codicon-check-all"></i>
             <span>${nodes.length}-step pipeline ready</span>
           </div>`
        : '';

    const nodeCards = nodes
      .map((node, i) => {
        const meta = MODE_META[node.mode] || MODE_META.work;
        const isDone = active && i < currentNode;
        const isCurrent = active && i === currentNode;
        return `
          <div class="wf-node${isDone ? ' wf-node--done' : isCurrent ? ' wf-node--active' : ''}" data-node-index="${i}">
            <div class="wf-node__header">
              <span class="wf-node__badge" style="--node-color:${meta.color}">
                <i class="codicon codicon-${isDone ? 'check' : meta.icon}"></i>
              </span>
              <div class="wf-node__meta">
                <select class="wf-node__mode-select select-inline" data-node-mode="${i}">
                  <option value="work"${node.mode === 'work' ? ' selected' : ''}>Work</option>
                  <option value="plan"${node.mode === 'plan' ? ' selected' : ''}>Plan</option>
                  <option value="review"${node.mode === 'review' ? ' selected' : ''}>Review</option>
                </select>
                <span class="wf-node__desc muted">${meta.description}</span>
              </div>
              <div class="wf-node__actions">
                <button class="icon-button" data-node-up="${i}" title="Move up"${i === 0 ? ' disabled' : ''}><i class="codicon codicon-arrow-up"></i></button>
                <button class="icon-button" data-node-down="${i}" title="Move down"${i === nodes.length - 1 ? ' disabled' : ''}><i class="codicon codicon-arrow-down"></i></button>
                <button class="icon-button danger" data-node-remove="${i}" title="Remove"><i class="codicon codicon-trash"></i></button>
              </div>
            </div>
            <input
              class="text-input wf-node__label"
              data-node-label="${i}"
              placeholder="Optional step label…"
              value="${escapeHtml(node.label || '')}"
            />
          </div>
          ${i < nodes.length - 1 ? '<div class="wf-connector"><i class="codicon codicon-arrow-down"></i></div>' : ''}`;
      })
      .join('');

    const canAdd = nodes.length < 3;

    return `
      <section class="sidebar-section">
        <div class="sidebar-section__header">
          <span>Pipeline</span>
          <button class="text-button" data-wf-reset="1" title="Clear and reset workflow">Reset</button>
        </div>
        ${statusBanner}
        <div class="wf-pipeline" id="wf-pipeline">
          ${nodeCards || '<div class="wf-empty"><i class="codicon codicon-symbol-misc"></i><span>No steps — add one below</span></div>'}
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
        <button class="primary-button wf-save-btn" data-wf-save="1"${nodes.length === 0 ? ' disabled' : ''}>
          <i class="codicon codicon-save"></i> Save & Activate
        </button>
      </div>`;
  }

  private bindWorkflow(container: HTMLElement): void {
    // Build a mutable draft of nodes from current state, updated on interactions
    const buildNodes = (): Array<{ mode: 'work' | 'plan' | 'review'; label: string }> => {
      const modeSelects = container.querySelectorAll<HTMLSelectElement>('[data-node-mode]');
      const labelInputs = container.querySelectorAll<HTMLInputElement>('[data-node-label]');
      const result: Array<{ mode: 'work' | 'plan' | 'review'; label: string }> = [];
      modeSelects.forEach((select, i) => {
        result.push({
          mode: select.value as 'work' | 'plan' | 'review',
          label: labelInputs[i]?.value.trim() || '',
        });
      });
      return result;
    };

    // Add step buttons
    container.querySelectorAll<HTMLElement>('[data-wf-add]').forEach((btn) => {
      btn.addEventListener('click', () => {
        const mode = btn.dataset.wfAdd as 'work' | 'plan' | 'review';
        const current = buildNodes();
        const updated = [...current, { mode, label: '' }];
        void this.workbenchService.saveWorkflow(updated);
      });
    });

    // Remove node
    container.querySelectorAll<HTMLElement>('[data-node-remove]').forEach((btn) => {
      btn.addEventListener('click', () => {
        const idx = Number(btn.dataset.nodeRemove);
        const current = buildNodes();
        current.splice(idx, 1);
        void this.workbenchService.saveWorkflow(current);
      });
    });

    // Move up
    container.querySelectorAll<HTMLElement>('[data-node-up]').forEach((btn) => {
      btn.addEventListener('click', () => {
        const idx = Number(btn.dataset.nodeUp);
        if (idx === 0) return;
        const current = buildNodes();
        [current[idx - 1], current[idx]] = [current[idx], current[idx - 1]];
        void this.workbenchService.saveWorkflow(current);
      });
    });

    // Move down
    container.querySelectorAll<HTMLElement>('[data-node-down]').forEach((btn) => {
      btn.addEventListener('click', () => {
        const idx = Number(btn.dataset.nodeDown);
        const current = buildNodes();
        if (idx >= current.length - 1) return;
        [current[idx], current[idx + 1]] = [current[idx + 1], current[idx]];
        void this.workbenchService.saveWorkflow(current);
      });
    });

    // Save & activate
    container.querySelector('[data-wf-save="1"]')?.addEventListener('click', () => {
      const nodes = buildNodes();
      void this.workbenchService.saveWorkflow(nodes);
    });

    // Reset
    container.querySelector('[data-wf-reset="1"]')?.addEventListener('click', () => {
      if (window.confirm('Clear the current workflow pipeline?')) {
        void this.workbenchService.resetWorkflow();
      }
    });
  }

  private renderScm(): string {
    const state = this.workbenchService.state;
    const rows =
      state.changes.length === 0
        ? '<div class="empty-state">No tracked changes.</div>'
        : state.changes
            .map(
              (change) => `
                <div class="change-row">
                  <button class="change-row__main" data-diff-path="${escapeHtml(change.path)}">
                    <strong>${escapeHtml(change.basename)}</strong>
                    <div class="muted">${escapeHtml(change.backup_time)}</div>
                  </button>
                  <button class="icon-button" data-revert-path="${escapeHtml(change.path)}" title="Revert"><i class="codicon codicon-discard"></i></button>
                </div>`,
            )
            .join('');
    return `
      <section class="sidebar-section">
        <div class="sidebar-section__header"><span>Changes</span><button class="text-button" data-refresh-changes="1">Refresh</button></div>
        <div class="sidebar-list">${rows}</div>
      </section>`;
  }

  private bindScm(container: HTMLElement): void {
    container.querySelector('[data-refresh-changes="1"]')?.addEventListener('click', () => {
      void this.workbenchService.refreshAll();
    });
    container.querySelectorAll<HTMLElement>('[data-diff-path]').forEach((button) => {
      button.addEventListener('click', () => {
        const path = button.dataset.diffPath;
        if (path) {
          void this.workbenchService.openDiffTab(path);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-revert-path]').forEach((button) => {
      button.addEventListener('click', () => {
        const path = button.dataset.revertPath;
        if (path && window.confirm(`Revert ${path}?`)) {
          void this.workbenchService.revertFile(path);
        }
      });
    });
  }

  private renderExtensions(): string {
    const state = this.workbenchService.state;
    const skills = Array.isArray(state.skills) ? state.skills : [];
    const cards =
      skills.length === 0
        ? '<div class="empty-state">No skills installed.</div>'
        : skills
            .map(
              (skill) => `
                <div class="extension-card">
                  <div class="extension-card__title">
                    <strong>${escapeHtml(skill.display_name || skill.name)}</strong>
                    <span class="badge${skill.enabled ? ' is-highlight' : ''}">${skill.enabled ? 'Enabled' : 'Disabled'}</span>
                  </div>
                  <div class="muted">${escapeHtml(skill.description || 'No description')}</div>
                  <div class="extension-card__actions">
                    <button class="text-button" data-preview-skill="${escapeHtml(skill.name)}">Preview</button>
                    <button class="text-button" data-toggle-skill="${escapeHtml(skill.name)}">${skill.enabled ? 'Disable' : 'Enable'}</button>
                    <button class="text-button" data-upgrade-skill="${escapeHtml(skill.name)}">Upgrade</button>
                    <button class="text-button danger" data-delete-skill="${escapeHtml(skill.name)}">Delete</button>
                  </div>
                </div>`,
            )
            .join('');
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

  private bindExtensions(container: HTMLElement): void {
    container.querySelector('#install-skill-button')?.addEventListener('click', () => {
      const input = container.querySelector<HTMLInputElement>('#skill-url-input');
      void this.workbenchService.installSkill(input?.value || '');
    });
    container.querySelectorAll<HTMLElement>('[data-toggle-skill]').forEach((button) => {
      button.addEventListener('click', () => {
        const name = button.dataset.toggleSkill;
        if (name) {
          void this.workbenchService.toggleSkill(name);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-preview-skill]').forEach((button) => {
      button.addEventListener('click', () => {
        const name = button.dataset.previewSkill;
        if (name) {
          void this.workbenchService.previewSkill(name);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-upgrade-skill]').forEach((button) => {
      button.addEventListener('click', () => {
        const name = button.dataset.upgradeSkill;
        if (name) {
          void this.workbenchService.upgradeSkill(name);
        }
      });
    });
    container.querySelectorAll<HTMLElement>('[data-delete-skill]').forEach((button) => {
      button.addEventListener('click', () => {
        const name = button.dataset.deleteSkill;
        if (name && window.confirm(`Delete skill "${name}"?`)) {
          void this.workbenchService.deleteSkill(name);
        }
      });
    });
  }

  private renderSettings(): string {
    const state = this.workbenchService.state;
    const themeOptions = THEME_OPTIONS.map(
      (theme) => `<option value="${theme}"${theme === state.theme ? ' selected' : ''}>${theme}</option>`,
    ).join('');
    const modelOptions = state.models
      .map((model, index) => `<option value="${index}"${index === state.currentModelIndex ? ' selected' : ''}>${escapeHtml(model.label || model.model)}</option>`)
      .join('');
    const remote = { ...DEFAULT_REMOTE_FORM, ...(state.remote.form || {}) };
    const selectedProfileId = matchProviderProfileId(state);
    const selectedProfile = state.providerProfiles.find((profile) => profile.id === selectedProfileId);
    const providerProfiles = state.providerProfiles
      .map(
        (profile) => `<option value="${escapeHtml(profile.id)}"${profile.id === selectedProfileId ? ' selected' : ''}>${escapeHtml(profile.label)} · ${escapeHtml(profile.model)}</option>`,
      )
      .join('');
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
        ${selectedProfile ? `<div class="profile-card"><strong>${escapeHtml(selectedProfile.label)}</strong><div class="muted">${escapeHtml(selectedProfile.description)}</div><div class="muted">${escapeHtml(selectedProfile.apibase)} · ${escapeHtml(selectedProfile.model)}</div></div>` : ''}
        <button id="apply-profile-button" class="text-button">Apply preset</button>
        <label class="field"><span>Provider</span><input id="provider-input" class="text-input" value="${escapeHtml(state.llmForm.provider || '')}" /></label>
        <label class="field"><span>Display name</span><input id="display-name-input" class="text-input" value="${escapeHtml(state.llmForm.name || '')}" /></label>
        <label class="field"><span>Model name</span><input id="model-name-input" class="text-input" value="${escapeHtml(state.llmForm.model || '')}" /></label>
        <label class="field"><span>Base URL</span><input id="base-url-input" class="text-input" value="${escapeHtml(state.llmForm.apibase || '')}" /></label>
        <label class="field"><span>API Key</span><input id="api-key-input" class="text-input" type="password" value="${escapeHtml(state.llmForm.apikey || '')}" /></label>
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
        <label class="toggle-inline"><input id="remote-enabled-input" type="checkbox"${remote.enabled ? ' checked' : ''} />Enable SSH</label>
        <label class="field"><span>Server name</span><input id="remote-name-input" class="text-input" value="${escapeHtml(remote.server_name || remote.name || '')}" /></label>
        <label class="field"><span>Host</span><input id="remote-host-input" class="text-input" value="${escapeHtml(remote.host)}" /></label>
        <label class="field"><span>Port</span><input id="remote-port-input" class="text-input" value="${remote.port}" /></label>
        <label class="field"><span>Username</span><input id="remote-user-input" class="text-input" value="${escapeHtml(remote.username)}" /></label>
        <label class="field"><span>Password</span><input id="remote-password-input" class="text-input" type="password" value="${escapeHtml(remote.password)}" /></label>
        <label class="field"><span>Key path</span><input id="remote-key-input" class="text-input" value="${escapeHtml(remote.key_path)}" /></label>
        <label class="field"><span>Working dir</span><input id="remote-cwd-input" class="text-input" value="${escapeHtml(remote.cwd)}" /></label>
        <button id="save-remote-button" class="primary-button">${state.remote.connected ? 'Reconnect remote' : 'Connect remote'}</button>
      </section>`;
  }

  private bindSettings(container: HTMLElement): void {
    container.querySelector<HTMLSelectElement>('#theme-select')?.addEventListener('change', (event) => {
      this.workbenchService.setTheme((event.currentTarget as HTMLSelectElement).value);
    });
    container.querySelector<HTMLSelectElement>('#model-select')?.addEventListener('change', (event) => {
      void this.workbenchService.switchModel(Number((event.currentTarget as HTMLSelectElement).value));
    });
    container.querySelector('#apply-profile-button')?.addEventListener('click', () => {
      const select = container.querySelector('#provider-profile-select') as HTMLSelectElement;
      void this.workbenchService.applyProviderProfile(select.value);
    });
    container.querySelector('#save-model-button')?.addEventListener('click', () => {
      void this.workbenchService.saveModelSettings({
        provider: (container.querySelector('#provider-input') as HTMLInputElement).value.trim(),
        name: (container.querySelector('#display-name-input') as HTMLInputElement).value.trim(),
        model: (container.querySelector('#model-name-input') as HTMLInputElement).value.trim(),
        apibase: (container.querySelector('#base-url-input') as HTMLInputElement).value.trim(),
        apikey: (container.querySelector('#api-key-input') as HTMLInputElement).value.trim(),
      });
    });
    container.querySelector('#save-workspace-button')?.addEventListener('click', () => {
      void this.workbenchService.saveWorkspaceSettings({
        name: (container.querySelector('#workspace-name-input') as HTMLInputElement).value.trim(),
        path: (container.querySelector('#workspace-path-input') as HTMLInputElement).value.trim(),
      });
    });
    container.querySelector<HTMLInputElement>('#workspace-name-input')?.addEventListener('input', (event) => {
      this.workbenchService.setWorkspaceDraft(
        (event.currentTarget as HTMLInputElement).value,
        (container.querySelector('#workspace-path-input') as HTMLInputElement).value,
      );
    });
    container.querySelector<HTMLInputElement>('#workspace-path-input')?.addEventListener('input', (event) => {
      this.workbenchService.setWorkspaceDraft(
        (container.querySelector('#workspace-name-input') as HTMLInputElement).value,
        (event.currentTarget as HTMLInputElement).value,
      );
    });
    container.querySelector('#pick-workspace-button')?.addEventListener('click', async () => {
      const picked = await this.workbenchService.pickWorkspacePath();
      if (picked) {
        const pathInput = container.querySelector('#workspace-path-input') as HTMLInputElement;
        const nameInput = container.querySelector('#workspace-name-input') as HTMLInputElement;
        pathInput.value = picked;
        nameInput.value = picked.split(/[\\/]/).filter(Boolean).pop() || '';
        this.workbenchService.setWorkspaceDraft(nameInput.value, pathInput.value);
      }
    });
    container.querySelector('#save-remote-button')?.addEventListener('click', () => {
      void this.workbenchService.saveRemoteSettings({
        enabled: (container.querySelector('#remote-enabled-input') as HTMLInputElement).checked,
        server_name: (container.querySelector('#remote-name-input') as HTMLInputElement).value.trim(),
        host: (container.querySelector('#remote-host-input') as HTMLInputElement).value.trim(),
        port: Number((container.querySelector('#remote-port-input') as HTMLInputElement).value || '22'),
        username: (container.querySelector('#remote-user-input') as HTMLInputElement).value.trim() || 'root',
        password: (container.querySelector('#remote-password-input') as HTMLInputElement).value,
        key_path: (container.querySelector('#remote-key-input') as HTMLInputElement).value.trim(),
        cwd: (container.querySelector('#remote-cwd-input') as HTMLInputElement).value.trim(),
      });
    });
  }
}
