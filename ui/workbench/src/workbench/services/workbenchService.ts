import { ApiClient } from '../../api';
import { Emitter } from '../../base/common/event';
import { Disposable, toDisposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import type { BootstrapPayload, ChatMessage, RemoteState } from '../../types';
import {
  createInitialWorkbenchState,
  DEFAULT_LLM_FORM,
  EditorTab,
  ModeId,
  THEME_OPTIONS,
  ViewId,
  WorkbenchState,
} from '../common/state';
import { NotificationService } from './notificationService';
import { INotificationService } from './serviceIds';

type QuickOpenResult = { kind: 'command' | 'file'; label: string; value: string };

export class WorkbenchService extends Disposable {
  private readonly api = new ApiClient();
  private readonly stateValue = createInitialWorkbenchState();
  private readonly changeEmitter = this._register(new Emitter<Readonly<WorkbenchState>>());
  readonly onDidChangeState = this.changeEmitter.event;
  private pollingTaskId: string | null = null;
  private static readonly AGENT_LOGS_STORAGE_KEY = 'generic-coder-show-detailed-agent-logs';
  private static readonly CONTROL_COMMANDS = new Set(['/ask', '/plan', '/build', '/work', '/review', '/clear']);

  private readonly notifications: NotificationService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.notifications = accessor.get(INotificationService);
  }

  get state(): Readonly<WorkbenchState> {
    return this.stateValue;
  }

  async start(): Promise<void> {
    const storedTheme = window.localStorage.getItem('generic-coder-theme');
    if (storedTheme && THEME_OPTIONS.includes(storedTheme as (typeof THEME_OPTIONS)[number])) {
      this.stateValue.theme = storedTheme;
    }
    const storedAgentLogs = window.localStorage.getItem(WorkbenchService.AGENT_LOGS_STORAGE_KEY);
    if (storedAgentLogs !== null) {
      this.stateValue.showDetailedAgentLogs = storedAgentLogs !== 'false';
    }
    this.applyTheme(false);
    await this.hydrateWorkspacePickerToken();
    await this.bootstrap();
    const interval = window.setInterval(() => {
      void this.refreshLightweightState();
    }, 5000);
    this._register(toDisposable(() => window.clearInterval(interval)));
  }

  private ensureTaskPlaceholder(preview: string, detailEvents: any[] = [], taskId?: string): void {
    const content = preview || '...';
    const nextMessage: ChatMessage = {
      role: 'agent-log',
      kind: 'agent-log',
      content,
      streaming: true,
      task_id: taskId || undefined,
      detail_events: detailEvents,
    };

    if (taskId) {
      const existingIndex = this.stateValue.messages.findIndex(
        (m) => m.kind === 'agent-log' && m.task_id === taskId,
      );
      if (existingIndex >= 0) {
        this.stateValue.messages[existingIndex] = nextMessage;
        this.stateValue.taskPlaceholderIndex = existingIndex;
        return;
      }
    }

    if (
      typeof this.stateValue.taskPlaceholderIndex === 'number'
      && this.stateValue.messages[this.stateValue.taskPlaceholderIndex]
    ) {
      this.stateValue.messages[this.stateValue.taskPlaceholderIndex] = nextMessage;
      return;
    }

    const lastIndex = this.stateValue.messages.length - 1;
    const lastMessage = this.stateValue.messages[lastIndex];
    if (lastMessage?.kind === 'agent-log' && lastMessage.streaming) {
      this.stateValue.messages[lastIndex] = nextMessage;
      this.stateValue.taskPlaceholderIndex = lastIndex;
      return;
    }

    this.stateValue.messages.push(nextMessage);
    this.stateValue.taskPlaceholderIndex = this.stateValue.messages.length - 1;
  }

  private discardTaskPlaceholder(): void {
    if (typeof this.stateValue.taskPlaceholderIndex !== 'number') {
      return;
    }
    const message = this.stateValue.messages[this.stateValue.taskPlaceholderIndex];
    if (message?.kind === 'agent-log') {
      this.stateValue.messages.splice(this.stateValue.taskPlaceholderIndex, 1);
    }
    this.stateValue.taskPlaceholderIndex = null;
  }

  private maybeResumePendingTask(data: BootstrapPayload): void {
    const pendingTaskId = data.pending_task?.task_id;
    if (!data.is_running || !pendingTaskId || this.pollingTaskId === pendingTaskId) {
      return;
    }

    this.stateValue.pendingTaskId = pendingTaskId;
    this.ensureTaskPlaceholder(
      data.pending_task?.preview || 'Starting task...',
      data.pending_task?.acp_events || [],
      pendingTaskId,
    );
    void this.pollTask(pendingTaskId);
  }

  setActiveView(view: ViewId): void {
    this.stateValue.activeView = view;
    this.emitChange();
  }

  toggleSidebar(): void {
    this.setActiveView(this.state.activeView === 'chat' ? 'explorer' : 'chat');
  }

  setActiveTab(tabId: string): void {
    this.stateValue.activeTabId = tabId;
    this.emitChange();
  }

  setQuickOpenVisible(visible: boolean): void {
    this.stateValue.quickOpenVisible = visible;
    this.emitChange();
  }

  setInputValue(value: string, emit = false): void {
    this.stateValue.inputValue = value;
    if (emit) {
      this.emitChange();
    }
  }

  insertMention(filePath: string, cursor?: number): string {
    const beforeCursor = this.stateValue.inputValue.slice(0, cursor ?? this.stateValue.inputValue.length);
    const afterCursor = this.stateValue.inputValue.slice(cursor ?? this.stateValue.inputValue.length);
    const atIndex = beforeCursor.lastIndexOf('@');
    this.stateValue.inputValue =
      atIndex >= 0
        ? `${beforeCursor.slice(0, atIndex)}@${filePath} ${afterCursor}`
        : `${beforeCursor}@${filePath} ${afterCursor}`;
    this.emitChange();
    return this.stateValue.inputValue;
  }

  async fetchMentionSuggestions(query: string): Promise<Array<{ name: string; path: string; rel: string }>> {
    if (!query.trim()) {
      return [];
    }
    return this.api.workspaceFiles(query, 5).catch(() => []);
  }

  async getQuickOpenFileResults(query: string): Promise<QuickOpenResult[]> {
    const fileRows = query
      ? (await this.api.workspaceFiles(query, 10)).map((item) => ({
          kind: 'file' as const,
          label: item.rel || item.name,
          value: item.path,
        }))
      : [];

    return fileRows;
  }

  async refreshAll(): Promise<void> {
    await Promise.all([
      this.loadWorkspaceTree(),
      this.loadSessions(),
      this.loadChanges(),
      this.loadSkills(),
      this.loadWorkflowState(),
      this.refreshPlanStatus(),
    ]);
    this.emitChange();
  }

  async switchModel(index: number): Promise<void> {
    try {
      await this.api.setModel(index);
      this.applyModelSettings(await this.api.settings());
      this.emitChange();
      this.notifications.notify(`Switched to ${this.stateValue.modelLabel}`);
    } catch (error) {
      this.notifyError(error, 'Failed to switch model');
    }
  }

  async saveModelSettings(payload: {
    provider: string;
    name: string;
    model: string;
    apibase: string;
    apikey: string;
    reasoning_effort?: string | null;
  }): Promise<void> {
    try {
      const request = {
        entry_key: this.state.llmForm.entry_key || '',
        session_type: this.state.llmForm.session_type || 'native_oai',
        protocol_preset: this.state.llmForm.protocol_preset || 'custom',
        api_mode: this.state.llmForm.api_mode || 'chat_completions',
        ...payload,
      };
      await this.api.saveLlmConfig(request);
      this.applyModelSettings(await this.api.settings());
      this.emitChange();
      this.notifications.notify('Model settings saved');
    } catch (error) {
      this.notifyError(error, 'Failed to save model settings');
    }
  }

  async applyProviderProfile(profileId: string): Promise<void> {
    const profile = this.stateValue.providerProfiles.find((entry) => entry.id === profileId);
    if (!profile) {
      this.notifications.notify('Unknown provider profile');
      return;
    }
    await this.saveModelSettings({
      provider: profile.provider,
      name: profile.label,
      model: profile.model,
      apibase: profile.apibase,
      apikey: this.stateValue.llmForm.apikey || '',
      reasoning_effort: profile.reasoning_effort || null,
    });
  }

  async inspectSessionCheckpoints(index: number): Promise<void> {
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
      this.notifyError(error, 'Failed to load restore points');
    }
  }

  async saveWorkspaceSettings(payload: { name: string; path: string }): Promise<void> {
    try {
      this.setWorkspaceDraft(payload.name, payload.path);
      const response = await this.api.saveWorkspace(payload);
      this.stateValue.workspace = {
        active: (response as any).active || null,
        workspaces: (response as any).workspaces || [],
        recent_folders: (response as any).recent_folders || [],
      };
      this.syncWorkspaceDraftFromActive();
      await this.loadWorkspaceTree();
      this.emitChange();
      this.notifications.notify('Workspace opened');
    } catch (error) {
      this.notifyError(error, 'Failed to save workspace');
    }
  }

  async pickWorkspacePath(): Promise<string | null> {
    if (!this.stateValue.workspacePickerToken) {
      this.notifications.notify('Workspace picker is unavailable');
      return null;
    }
    try {
      const payload = await this.api.pickWorkspace(this.stateValue.workspacePickerToken);
      return payload.path || null;
    } catch (error) {
      this.notifyError(error, 'Failed to open picker');
      return null;
    }
  }

  setWorkspaceDraft(name: string, path: string): void {
    this.stateValue.workspaceDraftName = name;
    this.stateValue.workspaceDraftPath = path;
  }

  toggleWorkspaceFolder(path: string): void {
    const collapsedPaths = new Set(this.stateValue.workspaceCollapsedPaths);
    if (collapsedPaths.has(path)) {
      collapsedPaths.delete(path);
    } else {
      collapsedPaths.add(path);
    }
    this.stateValue.workspaceCollapsedPaths = [...collapsedPaths];
    this.emitChange();
  }

  async saveRemoteSettings(payload: {
    enabled: boolean;
    server_name: string;
    host: string;
    port: number;
    username: string;
    password: string;
    key_path: string;
    cwd: string;
  }): Promise<void> {
    try {
      const remote = await this.api.connectRemote(payload);
      this.stateValue.remote = remote as RemoteState;
      this.emitChange();
      this.notifications.notify((remote as any).message || 'Remote state updated');
    } catch (error) {
      this.notifyError(error, 'Failed to update remote');
    }
  }

  async installSkill(url: string): Promise<void> {
    if (!url.trim()) {
      this.notifications.notify('Enter a skill URL first');
      return;
    }
    try {
      await this.api.installSkill(url.trim());
      await this.loadSkills();
      this.emitChange();
      this.notifications.notify('Skill installed');
    } catch (error) {
      this.notifyError(error, 'Failed to install skill');
    }
  }

  async toggleSkill(name: string): Promise<void> {
    try {
      await this.api.toggleSkill(name);
      await this.loadSkills();
      this.emitChange();
    } catch (error) {
      this.notifyError(error, 'Failed to toggle skill');
    }
  }

  async upgradeSkill(name: string): Promise<void> {
    try {
      await this.api.upgradeSkill(name);
      await this.loadSkills();
      this.emitChange();
      this.notifications.notify(`Upgraded ${name}`);
    } catch (error) {
      this.notifyError(error, 'Failed to upgrade skill');
    }
  }

  async deleteSkill(name: string): Promise<void> {
    try {
      await this.api.deleteSkill(name);
      await this.loadSkills();
      this.emitChange();
      this.notifications.notify(`Deleted ${name}`);
    } catch (error) {
      this.notifyError(error, 'Failed to delete skill');
    }
  }

  async previewSkill(name: string): Promise<void> {
    try {
      const preview = await this.api.previewSkill(name);
      this.stateValue.messages.push({
        role: 'assistant',
        content: `Skill preview: ${name}\n\n${preview.file || ''}\n\n${preview.content || ''}`,
      });
      this.stateValue.activeTabId = 'chat';
      this.ensureChatTab();
      this.emitChange();
      this.notifications.notify(`Loaded ${name} preview`);
    } catch (error) {
      this.notifyError(error, 'Failed to preview skill');
    }
  }

  async setMode(mode: ModeId): Promise<void> {
    try {
      await this.api.setMode(mode);
      this.stateValue.currentMode = mode;
      this.emitChange();
    } catch (error) {
      this.notifyError(error, 'Failed to set mode');
    }
  }

  async toggleMultiAgent(enabled: boolean, prompt: string): Promise<void> {
    if (enabled) {
      if (!prompt.trim()) {
        this.stateValue.multiAgentEnabled = false;
        this.emitChange();
        this.notifications.notify('Type a task before enabling multi-agent');
        return;
      }
      try {
        const suitability = await this.api.checkMultiAgentSuitable(prompt);
        if (!suitability.suitable) {
          this.stateValue.multiAgentEnabled = false;
          this.emitChange();
          this.notifications.notify(suitability.reason || 'Task is not suitable for multi-agent');
          return;
        }
      } catch (error) {
        this.stateValue.multiAgentEnabled = false;
        this.emitChange();
        this.notifyError(error, 'Failed to enable multi-agent');
        return;
      }
      if (this.stateValue.oneShotEnabled) {
        this.stateValue.oneShotEnabled = false;
        await this.api.setOneShot(false).catch(() => {});
      }
    }
    this.stateValue.multiAgentEnabled = enabled;
    await this.api.setMultiAgent(enabled).catch(() => {});
    this.emitChange();
  }

  async toggleOneShot(enabled: boolean): Promise<void> {
    if (enabled && this.stateValue.multiAgentEnabled) {
      this.stateValue.multiAgentEnabled = false;
      await this.api.setMultiAgent(false).catch(() => {});
    }
    this.stateValue.oneShotEnabled = enabled;
    await this.api.setOneShot(enabled).catch(() => {});
    this.emitChange();
  }

  async toggleWorkflowFollow(enabled: boolean): Promise<void> {
    if (enabled && this.stateValue.workflowNodes.length === 0) {
      this.notifications.notify('No workflow steps configured — add steps in the Workflow panel first');
      this.stateValue.workflowFollowEnabled = false;
      this.emitChange();
      return;
    }
    const result = await this.api.setWorkflowFollow(enabled).catch(() => ({ ok: false, reason: 'API error' }));
    if (!result.ok && enabled) {
      this.notifications.notify(result.reason || 'Cannot enable workflow follow');
      this.stateValue.workflowFollowEnabled = false;
    } else {
      this.stateValue.workflowFollowEnabled = enabled;
    }
    this.emitChange();
  }

  async toggleComputerUse(enabled: boolean): Promise<void> {
    if (enabled && !this.stateValue.computerUseAvailable) {
      this.notifications.notify('Computer Use is not available on this platform');
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

  async toggleYolo(enabled: boolean): Promise<void> {
    try {
      await this.api.setYolo(enabled);
      this.stateValue.yoloEnabled = enabled;
    } catch {
      this.stateValue.yoloEnabled = false;
    }
    this.emitChange();
  }

  async toggleAutoModel(enabled: boolean): Promise<void> {
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

  async cycleReasoningEffort(): Promise<void> {
    const order: Array<'off' | 'high' | 'max' | null> = [null, 'off', 'high', 'max'];
    const current = this.stateValue.reasoningEffort;
    const idx = order.indexOf(current);
    const next = order[(idx + 1) % order.length];
    try {
      await this.api.setReasoningEffort(next);
      this.stateValue.reasoningEffort = next;
    } catch { /* keep current */ }
    this.emitChange();
  }

  async setReasoningEffort(effort: 'off' | 'high' | 'max' | null): Promise<void> {
    try {
      await this.api.setReasoningEffort(effort);
      this.stateValue.reasoningEffort = effort;
    } catch { /* keep current */ }
    this.emitChange();
  }

  setShowDetailedAgentLogs(enabled: boolean): void {
    this.stateValue.showDetailedAgentLogs = enabled;
    window.localStorage.setItem(WorkbenchService.AGENT_LOGS_STORAGE_KEY, String(enabled));
    this.emitChange();
  }

  async toggleLoop(enabled: boolean): Promise<void> {
    if (enabled && !this.stateValue.loopAvailable) {
      this.notifications.notify('Task is not suitable for loop execution');
      this.stateValue.loopEnabled = false;
      this.emitChange();
      return;
    }
    this.stateValue.loopEnabled = enabled;
    await this.api.setLoop(enabled).catch(() => {});
    this.emitChange();
  }

  async checkLoopSuitability(prompt: string): Promise<void> {
    const suitable = this.isLoopSuitable(prompt);
    const changed = suitable !== this.stateValue.loopAvailable;
    this.stateValue.loopAvailable = suitable;
    if (!suitable && this.stateValue.loopEnabled) {
      this.stateValue.loopEnabled = false;
      await this.api.setLoop(false).catch(() => {});
    }
    if (changed) this.emitChange();
  }

  private isLoopSuitable(prompt: string): boolean {
    const trimmed = prompt.trim();
    if (trimmed.length < 10) return false;
    const lower = trimmed.toLowerCase();
    const loopKeywords = [
      // Chinese
      '循环', '反复', '重复', '不断', '持续', '每次', '每个', '每一个', '遍历', '迭代',
      '一直', '直到', '为止', '批量', '所有文件', '全部文件', '每个文件',
      '每隔', '定时', '监控', '监听', '实时',
      // English
      'loop', 'iterate', 'repeatedly', 'until', 'keep doing', 'keep running',
      'for each', 'for every', 'all files', 'every file', 'batch',
      'continuously', 'monitor', 'watch for', 'periodically', 'in a loop',
      'retry', 'repeat', 'cycle through', 'poll',
    ];
    return loopKeywords.some((kw) => lower.includes(kw));
  }

  async saveWorkflow(nodes: Array<{ mode: 'ask' | 'plan' | 'build' | 'review' | 'work'; label: string }>): Promise<void> {
    try {
      await this.api.saveWorkflow(nodes);
      await this.loadWorkflowState();
      this.emitChange();
      this.notifications.notify(nodes.length ? 'Workflow saved' : 'Workflow cleared');
    } catch (error) {
      this.notifyError(error, 'Failed to save workflow');
    }
  }

  async resetWorkflow(): Promise<void> {
    try {
      await this.api.resetWorkflow();
      await this.loadWorkflowState();
      this.emitChange();
      this.notifications.notify('Workflow reset');
    } catch (error) {
      this.notifyError(error, 'Failed to reset workflow');
    }
  }

  async sendPrompt(rawPrompt: string): Promise<void> {
    const prompt = rawPrompt.trim();
    if (!prompt) {
      return;
    }
    // Allow slash commands even when running; block non-slash input
    if (this.stateValue.isRunning && !prompt.startsWith('/')) {
      return;
    }
    const isControlCommand = WorkbenchService.CONTROL_COMMANDS.has(prompt);
    if (!isControlCommand) {
      this.stateValue.messages.push({
        role: 'user',
        content: prompt,
        mode: this.stateValue.currentMode,
        timestamp: Date.now(),
      });
    }
    this.ensureChatTab();
    this.stateValue.activeTabId = 'chat';
    this.stateValue.inputValue = '';
    this.stateValue.isRunning = true;
    this.emitChange();

    try {
      const payload = await this.api.chat(prompt);
      if (payload.handled) {
        this.stateValue.messages = (payload.messages as ChatMessage[]) || this.stateValue.messages;
        this.stateValue.isRunning = false;
        await this.syncSessionState();
        this.emitChange();
        if (payload.notice) {
          this.notifications.notify(payload.notice);
        }
        return;
      }
      if (!payload.task_id) {
        throw new Error(payload.error || 'Task creation failed');
      }
      this.stateValue.pendingTaskId = payload.task_id;
      if (!prompt.startsWith('/')) {
        this.ensureTaskPlaceholder('Starting task...', [], payload.task_id);
      }
      this.emitChange();
      await this.pollTask(payload.task_id);
    } catch (error) {
      this.discardTaskPlaceholder();
      this.stateValue.isRunning = false;
      this.emitChange();
      this.notifyError(error, 'Failed to send prompt');
    }
  }

  async stopTask(): Promise<void> {
    try {
      await this.api.stop();
    } catch {
      // best effort
    }
    this.stateValue.isRunning = false;
    this.stateValue.pendingTaskId = null;
    this.stateValue.taskPlaceholderIndex = null;
    this.emitChange();
  }

  async restoreSession(index: number, checkpoint?: number): Promise<void> {
    try {
      const payload = await this.api.restoreSession(index, checkpoint);
      this.stateValue.messages = (payload.messages as ChatMessage[]) || [];
      this.stateValue.activeTabId = 'chat';
      this.ensureChatTab();
      await this.syncSessionState();
      this.emitChange();
      this.notifications.notify(
        checkpoint ? `Restored session #${index} @ checkpoint ${checkpoint}` : `Restored session #${index}`,
      );
    } catch (error) {
      this.notifyError(error, 'Failed to restore session');
    }
  }

  async forkSession(index: number, checkpoint?: number): Promise<void> {
    try {
      const payload = await this.api.forkSession(index, checkpoint);
      this.stateValue.messages = (payload.messages as ChatMessage[]) || [];
      this.stateValue.activeTabId = 'chat';
      this.ensureChatTab();
      await this.syncSessionState();
      this.emitChange();
      this.notifications.notify(
        checkpoint ? `Forked session #${index} @ checkpoint ${checkpoint}` : `Forked session #${index}`,
      );
    } catch (error) {
      this.notifyError(error, 'Failed to fork session');
    }
  }

  async deleteSession(index: number): Promise<void> {
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
        payload.was_active ? `Deleted current session #${index}` : `Deleted session #${index}`,
      );
    } catch (error) {
      this.notifyError(error, 'Failed to delete session');
    }
  }

  async openPreviewTab(filePath: string): Promise<void> {
    try {
      const preview = await this.api.workspacePreview(filePath);
      const existing = this.stateValue.tabs.find(
        (tab): tab is Extract<EditorTab, { kind: 'preview' }> => tab.kind === 'preview' && tab.path === preview.path,
      );
      const nextTab: EditorTab = existing || {
        id: `preview:${preview.path}`,
        title: preview.rel || preview.name,
        kind: 'preview',
        path: preview.path,
        preview,
      };
      if (!existing) {
        this.stateValue.tabs.push(nextTab);
      } else {
        existing.preview = preview;
      }
      this.stateValue.activeTabId = nextTab.id;
      this.emitChange();
    } catch (error) {
      this.notifyError(error, 'Failed to open preview');
    }
  }

  async openDiffTab(filePath: string): Promise<void> {
    try {
      const payload = await this.api.diff(filePath);
      const diffText = payload.has_changes ? payload.diff : 'No changes.';
      const existing = this.stateValue.tabs.find(
        (tab): tab is Extract<EditorTab, { kind: 'diff' }> => tab.kind === 'diff' && tab.path === filePath,
      );
      const nextTab: EditorTab = existing || {
        id: `diff:${filePath}`,
        title: `Diff · ${filePath.split(/[\\/]/).pop() || filePath}`,
        kind: 'diff',
        path: filePath,
        diff: diffText,
      };
      if (!existing) {
        this.stateValue.tabs.push(nextTab);
      } else {
        existing.diff = diffText;
      }
      this.stateValue.activeTabId = nextTab.id;
      this.emitChange();
    } catch (error) {
      this.notifyError(error, 'Failed to show diff');
    }
  }

  closeTab(id: string): void {
    this.stateValue.tabs = this.stateValue.tabs.filter((tab) => tab.id !== id);
    if (this.stateValue.activeTabId === id) {
      this.stateValue.activeTabId = 'chat';
    }
    this.emitChange();
  }

  async revertFile(filePath: string): Promise<void> {
    try {
      await this.api.revert(filePath);
      await this.loadChanges();
      this.emitChange();
      this.notifications.notify(`Reverted ${filePath}`);
    } catch (error) {
      this.notifyError(error, 'Failed to revert file');
    }
  }

  getEditorTheme(): 'vs' | 'vs-dark' {
    return this.stateValue.theme === 'daybreak' || this.stateValue.theme === 'paperink' ? 'vs' : 'vs-dark';
  }

  applyTheme(persist = true): void {
    document.documentElement.dataset.theme = this.stateValue.theme;
    if (persist) {
      window.localStorage.setItem('generic-coder-theme', this.stateValue.theme);
    }
    this.emitChange();
  }

  setTheme(theme: string): void {
    this.stateValue.theme = theme;
    this.applyTheme();
    void this.api.setTheme(theme);
  }

  private async hydrateWorkspacePickerToken(): Promise<void> {
    const cached = window.sessionStorage.getItem('generic-coder-picker-token');
    if (cached) {
      this.stateValue.workspacePickerToken = cached;
      return;
    }
    try {
      this.stateValue.workspacePickerToken = await this.api.pickerToken();
      if (this.stateValue.workspacePickerToken) {
        window.sessionStorage.setItem('generic-coder-picker-token', this.stateValue.workspacePickerToken);
      }
    } catch {
      this.stateValue.workspacePickerToken = '';
    }
  }

  private async bootstrap(): Promise<void> {
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
        this.refreshPlanStatus(),
      ]);
      this.emitChange();
    } catch (error) {
      this.notifyError(error, 'Failed to bootstrap workbench');
    }
  }

  private applyBootstrap(data: BootstrapPayload): void {
    const previousCurrentSessionIndex = this.stateValue.currentSessionIndex;
    this.stateValue.messages = data.messages || [];
    this.stateValue.isRunning = Boolean(data.is_running);
    this.stateValue.pendingTaskId = data.pending_task?.task_id || null;
    this.stateValue.taskPlaceholderIndex = null;
    if (data.is_running && data.pending_task?.task_id) {
      this.ensureTaskPlaceholder(
        data.pending_task.preview || 'Starting task...',
        data.pending_task.acp_events || [],
        data.pending_task.task_id,
      );
    }
    this.applyModelSettings(data);
    this.stateValue.providerProfiles = data.provider_profiles || this.stateValue.providerProfiles;
    this.stateValue.workspace = data.workspace || this.stateValue.workspace;
    this.syncWorkspaceDraftFromActive();
    this.stateValue.remote = data.remote || this.stateValue.remote;
    this.stateValue.currentMode = (data.mode as ModeId) || 'build';
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
    this.stateValue.reasoningEffort = (effort === 'off' || effort === 'high' || effort === 'max') ? effort : null;
    this.stateValue.autoModelEnabled = Boolean(data.auto_model_enabled);
    this.stateValue.autoRoute = data.auto_route || null;
    this.stateValue.currentSessionIndex = data.current_session?.index ?? null;
    this.stateValue.currentSessionActiveCheckpoint = data.current_session?.active_checkpoint ?? null;
    this.stateValue.currentSessionCheckpoints = data.current_session?.checkpoints || [];
    this.stateValue.sessionUsage = data.current_session?.usage_totals || null;
    this.stateValue.lastUsage = data.current_session?.last_usage || null;
    if (
      this.stateValue.currentSessionIndex === null
      || this.stateValue.checkpointPanelSessionIndex === null
      || this.stateValue.checkpointPanelSessionIndex === previousCurrentSessionIndex
      || this.stateValue.checkpointPanelSessionIndex === this.stateValue.currentSessionIndex
    ) {
      this.stateValue.checkpointPanelSessionIndex = this.stateValue.currentSessionIndex;
      this.stateValue.checkpointPanelEntries = this.stateValue.currentSessionCheckpoints;
    }
    if (data.picker_token && !this.stateValue.workspacePickerToken) {
      this.stateValue.workspacePickerToken = data.picker_token;
      window.sessionStorage.setItem('generic-coder-picker-token', data.picker_token);
    }
    if (data.theme) {
      this.stateValue.theme = data.theme;
      document.documentElement.dataset.theme = data.theme;
    }
    this.ensureChatTab();
  }

  private applyModelSettings(data: Pick<BootstrapPayload, 'llm_form' | 'models' | 'model' | 'model_index'>): void {
    const models = Array.isArray(data.models?.models) ? data.models.models : [];
    const currentIndex = Number.isInteger(data.model_index)
      ? Number(data.model_index)
      : Number.isInteger(data.models?.current_index)
        ? Number(data.models?.current_index)
        : 0;
    const currentModel = models[currentIndex];

    this.stateValue.currentModelIndex = currentIndex;
    this.stateValue.models = models;
    this.stateValue.modelLabel = data.model || currentModel?.label || currentModel?.model || 'Model offline';
    this.stateValue.llmForm = { ...DEFAULT_LLM_FORM, ...(data.llm_form || this.stateValue.llmForm) };
  }

  private syncWorkspaceDraftFromActive(): void {
    this.stateValue.workspaceDraftName = this.stateValue.workspace.active?.name || '';
    this.stateValue.workspaceDraftPath = this.stateValue.workspace.active?.path || '';
  }

  private async refreshLightweightState(): Promise<void> {
    if (this.stateValue.isRunning && this.stateValue.pendingTaskId) {
      return;
    }
    if (this.stateValue.isRunning) {
      const bootstrap = await this.api.bootstrap().catch(() => ({} as BootstrapPayload));
      this.applyBootstrap(bootstrap);
      this.maybeResumePendingTask(bootstrap);
      this.emitChange();
      return;
    }
    await Promise.allSettled([this.loadChanges(), this.loadSessions(), this.refreshPlanStatus()]);
    this.emitChange();
  }

  private async loadWorkspaceTree(): Promise<void> {
    this.stateValue.workspaceTree = await this.api.workspaceTree().catch(() => []);
    const directoryPaths = new Set(
      this.stateValue.workspaceTree.filter((entry) => entry.type === 'dir').map((entry) => entry.path),
    );
    this.stateValue.workspaceCollapsedPaths = this.stateValue.workspaceCollapsedPaths.filter((path) =>
      directoryPaths.has(path),
    );
  }

  private async loadSessions(): Promise<void> {
    this.stateValue.sessions = await this.api.sessions().catch(() => []);
  }

  private async syncSessionState(): Promise<void> {
    const bootstrap = await this.api.bootstrap().catch(() => ({} as BootstrapPayload));
    this.applyBootstrap(bootstrap);
    await this.loadSessions();
  }

  private async loadChanges(): Promise<void> {
    this.stateValue.changes = await this.api.changes().catch(() => []);
  }

  private async loadSkills(): Promise<void> {
    this.stateValue.skills = await this.api.skills().catch(() => []);
  }

  private async loadWorkflowState(): Promise<void> {
    try {
      const workflow = await this.api.workflow();
      this.stateValue.workflowNodes = Array.isArray(workflow.nodes) ? workflow.nodes : [];
      this.stateValue.workflowActive = Boolean(workflow.active);
      this.stateValue.workflowCurrentNode = workflow.current_node || 0;
      this.stateValue.currentMode = await this.api.mode();
    } catch {
      // retain last state
    }
  }

  private async refreshPlanStatus(): Promise<void> {
    try {
      const status = await this.api.planStatus();
      this.stateValue.planRemaining = status.remaining;
    } catch {
      this.stateValue.planRemaining = -1;
    }
  }

  private async pollTask(taskId: string): Promise<void> {
    if (this.pollingTaskId === taskId) {
      return;
    }
    this.pollingTaskId = taskId;
    try {
      while (true) {
        const payload = await this.api.task(taskId);
        this.ensureTaskPlaceholder(payload.preview || '...', payload.acp_events || [], taskId);
        if (payload.done && typeof this.stateValue.taskPlaceholderIndex === 'number') {
          this.stateValue.messages[this.stateValue.taskPlaceholderIndex] = {
            ...this.stateValue.messages[this.stateValue.taskPlaceholderIndex],
            role: 'agent-log',
            kind: 'agent-log',
            content: payload.final || payload.preview || 'Done',
            streaming: false,
            task_id: taskId,
            detail_events: payload.acp_events || [],
          };
        }
        this.emitChange();
        if (payload.done) {
          // Normalize token usage across provider formats
          const promptTokens = payload.usage?.prompt_tokens ?? payload.usage?.input_tokens;
          const completionTokens = payload.usage?.completion_tokens ?? payload.usage?.output_tokens;
          if (typeof promptTokens === 'number' || typeof completionTokens === 'number') {
            this.stateValue.lastUsage = {
              prompt_tokens: promptTokens || 0,
              completion_tokens: completionTokens || 0,
              cached_tokens: payload.usage?.prompt_cache_hit_tokens ?? payload.usage?.cached_tokens ?? 0,
            };
          }
          break;
        }
        await new Promise((resolve) => window.setTimeout(resolve, 300));
      }
      this.stateValue.pendingTaskId = null;
      this.stateValue.taskPlaceholderIndex = null;
      this.stateValue.isRunning = false;
      if (this.pollingTaskId === taskId) {
        this.pollingTaskId = null;
      }
      this.applyBootstrap(await this.api.bootstrap().catch(() => ({} as BootstrapPayload)));
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
      this.notifyError(error, 'Failed to poll task');
    }
  }

  private ensureChatTab(): void {
    if (!this.stateValue.tabs.find((tab) => tab.id === 'chat')) {
      this.stateValue.tabs.unshift({ id: 'chat', title: 'Chat', kind: 'chat' });
    }
  }

  private notifyError(error: unknown, fallback: string): void {
    this.notifications.notify(error instanceof Error ? error.message : fallback);
  }

  private emitChange(): void {
    this.changeEmitter.fire(this.state);
  }
}
