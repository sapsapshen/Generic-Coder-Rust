import { Disposable, toDisposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import type { ModeId } from '../common/state';
import { CommandService } from '../services/commandService';
import { LayoutService } from '../services/layoutService';
import { ICommandService, ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

export class ComposerPart extends Disposable {
  private mentionSuggestions: Array<{ name: string; path: string; rel: string }> = [];
  private loopCheckTimer: ReturnType<typeof setTimeout> | null = null;
  private slashSelectedIndex = 0;
  private commandHistory: string[] = [];
  private historyIndex = -1;
  private historyDraft = '';
  private autoDetectedMode: string | null = null;
  private modeUserOverridden = false;

  private static readonly SLASH_COMMANDS = [
    { cmd: '/new',      desc: 'Start a fresh session (clears context)' },
    { cmd: '/fork',     desc: 'Fork the current session into a new branch' },
    { cmd: '/continue', desc: '/continue <n>  — restore and resume session #n' },
    { cmd: '/ask',      desc: 'Switch to Ask mode (Q&A and code explanation)' },
    { cmd: '/plan',     desc: 'Switch to Plan mode (explore without modifying files)' },
    { cmd: '/build',    desc: 'Switch to Build mode (implement and execute)' },
    { cmd: '/review',   desc: 'Switch to Review mode (audit code for issues)' },
    { cmd: '/clear',    desc: 'Clear error memory and avoidance hints' },
  ];

  private readonly layoutService: LayoutService;
  private readonly workbenchService: WorkbenchService;
  private readonly commandService: CommandService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.layoutService = accessor.get(ILayoutService);
    this.workbenchService = accessor.get(IWorkbenchService);
    this.commandService = accessor.get(ICommandService);
    this.bind();
    try {
      this.commandHistory = JSON.parse(window.localStorage.getItem('gc-cmd-history') || '[]');
    } catch {
      this.commandHistory = [];
    }
    this._register(this.workbenchService.onDidChangeState(() => this.render()));
    this.render();
  }

  static detectMode(text: string): string | null {
    const t = text.toLowerCase().trim();
    if (!t) return null;
    if (t.startsWith('/ask') || /\b(explain|what is|what does|how does|why does|understand|describe)\b/.test(t)) return 'ask';
    if (t.startsWith('/review') || /\b(review|audit|check|inspect|analyse|analyze|security)\b/.test(t)) return 'review';
    if (t.startsWith('/plan') || /\b(plan|design|architecture|outline|breakdown|roadmap)\b/.test(t)) return 'plan';
    if (t.startsWith('/build') || /\b(implement|build|create|add|fix|refactor|write|generate|make)\b/.test(t)) return 'build';
    return null;
  }

  private updateAutoDetectedMode(input: HTMLTextAreaElement): void {
    if (this.modeUserOverridden) return;
    const detected = ComposerPart.detectMode(input.value);
    const modeSelect = this.layoutService.getElement<HTMLSelectElement>('mode-select');
    if (detected && detected !== this.autoDetectedMode) {
      this.autoDetectedMode = detected;
      modeSelect.value = detected;
      modeSelect.dataset.autoDetected = detected;
      modeSelect.title = `Auto-detected: ${detected}`;
    } else if (!detected && this.autoDetectedMode) {
      this.autoDetectedMode = null;
      delete modeSelect.dataset.autoDetected;
      modeSelect.title = '';
      modeSelect.value = this.workbenchService.state.currentMode;
    }
  }

  private async handleSend(input: HTMLTextAreaElement): Promise<void> {
    const value = input.value.trim();
    if (!value) return;
    // Save to history
    if (this.commandHistory[0] !== value) {
      this.commandHistory.unshift(value);
      if (this.commandHistory.length > 100) this.commandHistory.pop();
      try { window.localStorage.setItem('gc-cmd-history', JSON.stringify(this.commandHistory)); } catch {}
    }
    this.historyIndex = -1;
    this.historyDraft = '';
    // Apply auto-detected mode if not overridden and different from current
    if (!this.modeUserOverridden && this.autoDetectedMode && this.autoDetectedMode !== this.workbenchService.state.currentMode) {
      await this.workbenchService.setMode(this.autoDetectedMode as any);
    }
    // Reset auto-detection
    this.autoDetectedMode = null;
    this.modeUserOverridden = false;
    const modeSelect = this.layoutService.getElement<HTMLSelectElement>('mode-select');
    delete modeSelect.dataset.autoDetected;
    modeSelect.title = '';
    await this.workbenchService.sendPrompt(input.value);
  }

  private bind(): void {
    const input = this.layoutService.getElement<HTMLTextAreaElement>('prompt-input');
    const sendButton = this.layoutService.getElement<HTMLButtonElement>('send-button');
    const modeSelect = this.layoutService.getElement<HTMLSelectElement>('mode-select');
    const multiAgentToggle = this.layoutService.getElement<HTMLInputElement>('multi-agent-toggle');
    const oneShotToggle = this.layoutService.getElement<HTMLInputElement>('one-shot-toggle');
    const agentLogsToggle = this.layoutService.getElement<HTMLInputElement>('agent-logs-toggle');
    const loopToggle = this.layoutService.getElement<HTMLInputElement>('loop-toggle');
    const workflowFollowToggle = this.layoutService.getElement<HTMLInputElement>('workflow-follow-toggle');
    const computerUseToggle = this.layoutService.getElement<HTMLInputElement>('computer-use-toggle');
    const yoloToggle = this.layoutService.getElement<HTMLInputElement>('yolo-toggle');
    const autoModelToggle = this.layoutService.getElement<HTMLInputElement>('auto-model-toggle');
    const effortGroup = this.layoutService.getElement<HTMLDivElement>('effort-group');

    // YOLO warning dismiss button
    const yoloWarningOff = this.layoutService.getElement<HTMLButtonElement>('yolo-warning-off');
    yoloWarningOff.addEventListener('click', () => {
      void this.workbenchService.toggleYolo(false);
    });

    input.addEventListener('input', async () => {
      this.workbenchService.setInputValue(input.value);
      this.updateSlashHints(input);
      this.updateAutoDetectedMode(input);
      await this.refreshMentionSuggestions();
      this.scheduleLoopCheck(input.value);
    });

    input.addEventListener('keydown', async (event) => {
      const slashHints = this.layoutService.getElement<HTMLDivElement>('slash-hints');
      if (!slashHints.hidden) {
        if (event.key === 'ArrowDown') {
          event.preventDefault();
          this.slashSelectedIndex = Math.min(this.slashSelectedIndex + 1, this.visibleSlashCommands(input.value).length - 1);
          this.renderSlashHints(input, slashHints);
          return;
        }
        if (event.key === 'ArrowUp') {
          event.preventDefault();
          this.slashSelectedIndex = Math.max(this.slashSelectedIndex - 1, 0);
          this.renderSlashHints(input, slashHints);
          return;
        }
        if (event.key === 'Enter' || event.key === 'Tab') {
          const cmds = this.visibleSlashCommands(input.value);
          if (cmds[this.slashSelectedIndex]) {
            event.preventDefault();
            input.value = cmds[this.slashSelectedIndex].cmd + ' ';
            this.workbenchService.setInputValue(input.value);
            slashHints.hidden = true;
            return;
          }
        }
        if (event.key === 'Escape') {
          event.preventDefault();
          slashHints.hidden = true;
          return;
        }
      }

      // History navigation when not showing slash hints
      if (slashHints.hidden) {
        if (event.key === 'ArrowUp') {
          const lines = input.value.split('\n');
          const isFirstLine = input.selectionStart <= lines[0].length;
          if (isFirstLine && this.commandHistory.length > 0) {
            event.preventDefault();
            if (this.historyIndex < 0) {
              this.historyDraft = input.value;
            }
            this.historyIndex = Math.min(this.historyIndex + 1, this.commandHistory.length - 1);
            input.value = this.commandHistory[this.historyIndex];
            this.workbenchService.setInputValue(input.value);
            return;
          }
        }
        if (event.key === 'ArrowDown') {
          if (this.historyIndex >= 0) {
            const lines = input.value.split('\n');
            const isLastLine = input.selectionStart >= input.value.length - lines[lines.length - 1].length;
            if (isLastLine) {
              event.preventDefault();
              this.historyIndex--;
              if (this.historyIndex < 0) {
                input.value = this.historyDraft;
              } else {
                input.value = this.commandHistory[this.historyIndex];
              }
              this.workbenchService.setInputValue(input.value);
              return;
            }
          }
        }
      }

      if (event.key === 'Enter' && !event.shiftKey && !event.metaKey && !event.ctrlKey) {
        event.preventDefault();
        slashHints.hidden = true;
        await this.handleSend(input);
        return;
      }

      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === 'enter') {
        event.preventDefault();
        slashHints.hidden = true;
        await this.handleSend(input);
        return;
      }
      if (event.key === 'Tab' && this.mentionSuggestions[0] && input.value.includes('@')) {
        event.preventDefault();
        const nextValue = this.workbenchService.insertMention(this.mentionSuggestions[0].rel || this.mentionSuggestions[0].path, input.selectionStart);
        input.value = nextValue;
      }
    });

    const keydownHandler = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (mod && key === 'p') {
        event.preventDefault();
        void this.commandService.executeCommand('workbench.action.quickOpen');
        return;
      }
      if (mod && event.shiftKey && key === 'n') {
        event.preventDefault();
        void this.commandService.executeCommand('workbench.action.newChat');
        return;
      }
      if (event.key === 'Escape') {
        void this.commandService.executeCommand('workbench.action.closeQuickOpen');
      }
    };
    document.addEventListener('keydown', keydownHandler);
    this._register(
      toDisposable(() => {
        document.removeEventListener('keydown', keydownHandler);
      }),
    );

    sendButton.addEventListener('click', () => {
      void this.handleSend(input);
    });
    modeSelect.addEventListener('change', () => {
      this.modeUserOverridden = true;
      this.autoDetectedMode = null;
      delete modeSelect.dataset.autoDetected;
      modeSelect.title = '';
      void this.workbenchService.setMode(modeSelect.value as ModeId);
    });
    multiAgentToggle.addEventListener('change', () => {
      void this.workbenchService.toggleMultiAgent(multiAgentToggle.checked, input.value);
    });
    oneShotToggle.addEventListener('change', () => {
      void this.workbenchService.toggleOneShot(oneShotToggle.checked);
    });
    agentLogsToggle.addEventListener('change', () => {
      this.workbenchService.setShowDetailedAgentLogs(agentLogsToggle.checked);
    });
    loopToggle.addEventListener('change', () => {
      void this.workbenchService.toggleLoop(loopToggle.checked);
    });
    workflowFollowToggle.addEventListener('change', () => {
      void this.workbenchService.toggleWorkflowFollow(workflowFollowToggle.checked);
    });
    computerUseToggle.addEventListener('change', () => {
      void this.workbenchService.toggleComputerUse(computerUseToggle.checked);
    });
    yoloToggle.addEventListener('change', () => {
      void this.workbenchService.toggleYolo(yoloToggle.checked);
    });
    autoModelToggle.addEventListener('change', () => {
      void this.workbenchService.toggleAutoModel(autoModelToggle.checked);
    });
    effortGroup.querySelectorAll<HTMLButtonElement>('.effort-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const effort = btn.dataset.effort as 'off' | 'high' | 'max';
        const current = this.workbenchService.state.reasoningEffort;
        void this.workbenchService.setReasoningEffort(current === effort ? null : effort);
      });
    });
  }

  render(): void {
    const state = this.workbenchService.state;
    const input = this.layoutService.getElement<HTMLTextAreaElement>('prompt-input');
    const modeSelect = this.layoutService.getElement<HTMLSelectElement>('mode-select');
    const multiAgentToggle = this.layoutService.getElement<HTMLInputElement>('multi-agent-toggle');
    const oneShotToggle = this.layoutService.getElement<HTMLInputElement>('one-shot-toggle');
    const agentLogsToggle = this.layoutService.getElement<HTMLInputElement>('agent-logs-toggle');
    const loopToggle = this.layoutService.getElement<HTMLInputElement>('loop-toggle');
    const workflowFollowToggle = this.layoutService.getElement<HTMLInputElement>('workflow-follow-toggle');
    const workflowFollowLabel = this.layoutService.getElement<HTMLSpanElement>('workflow-follow-label');
    const computerUseToggle = this.layoutService.getElement<HTMLInputElement>('computer-use-toggle');
    const yoloToggle = this.layoutService.getElement<HTMLInputElement>('yolo-toggle');
    const autoModelToggle = this.layoutService.getElement<HTMLInputElement>('auto-model-toggle');
    const effortGroup = this.layoutService.getElement<HTMLDivElement>('effort-group');
    const sendButton = this.layoutService.getElement<HTMLButtonElement>('send-button');
    const meta = this.layoutService.getElement<HTMLSpanElement>('composer-meta');

    if (input.value !== state.inputValue) {
      input.value = state.inputValue;
    }
    modeSelect.value = state.currentMode;
    multiAgentToggle.checked = state.multiAgentEnabled;
    oneShotToggle.checked = state.oneShotEnabled;
    agentLogsToggle.checked = state.showDetailedAgentLogs;

    loopToggle.disabled = !state.loopAvailable;
    loopToggle.checked = state.loopEnabled;
    loopToggle.title = state.loopAvailable
      ? 'Repeat this task in a loop'
      : 'Loop not available for this task';

    workflowFollowToggle.checked = state.workflowFollowEnabled;
    workflowFollowToggle.disabled = state.workflowNodes.length === 0;
    workflowFollowToggle.title = state.workflowNodes.length === 0
      ? 'No workflow steps configured — open the Workflow panel'
      : 'Strictly follow the workflow pipeline order';

    if (state.workflowNodes.length > 0) {
      workflowFollowLabel.textContent = `Workflow: ${state.workflowNodes.map((n) => n.mode).join(' → ')}`;
    } else {
      workflowFollowLabel.textContent = 'Workflow';
    }

    computerUseToggle.checked = state.computerUseEnabled;
    computerUseToggle.disabled = !state.computerUseAvailable;
    computerUseToggle.title = state.computerUseAvailable
      ? 'Allow AI to control mouse, keyboard and screen'
      : 'Computer Use is not available on this platform';

    yoloToggle.checked = state.yoloEnabled;
    yoloToggle.title = state.yoloEnabled
      ? 'YOLO: AI executes all actions autonomously without confirmation'
      : 'Enable YOLO mode for fully autonomous execution';

    // Show/hide YOLO warning banner
    const yoloWarning = this.layoutService.getElement<HTMLDivElement>('yolo-warning');
    yoloWarning.hidden = !state.yoloEnabled;

    autoModelToggle.checked = state.autoModelEnabled;
    autoModelToggle.title = state.autoModelEnabled
      ? 'DeepSeek-style automatic model + reasoning routing is active'
      : 'Automatically route each turn to the best model';

    effortGroup.querySelectorAll<HTMLButtonElement>('.effort-btn').forEach((btn) => {
      const effort = btn.dataset.effort;
      btn.classList.toggle('effort-btn--active', state.reasoningEffort === effort);
      btn.disabled = state.autoModelEnabled;
      btn.title = effort === 'off' ? 'Disable reasoning' : effort === 'high' ? 'High reasoning effort' : 'Maximum reasoning effort';
    });

    const canSend = !state.isRunning;
    sendButton.disabled = !canSend;
    input.disabled = state.isRunning;
    meta.textContent = state.isRunning
      ? 'Task running'
      : state.autoModelEnabled && state.autoRoute
        ? `Auto -> ${state.autoRoute.model}${state.autoRoute.reasoning_effort ? ` / ${state.autoRoute.reasoning_effort}` : ''}`
        : state.autoModelEnabled
          ? 'Auto routing enabled'
          : '';
  }

  private scheduleLoopCheck(prompt: string): void {
    if (this.loopCheckTimer !== null) {
      clearTimeout(this.loopCheckTimer);
    }
    this.loopCheckTimer = setTimeout(() => {
      this.loopCheckTimer = null;
      void this.workbenchService.checkLoopSuitability(prompt);
    }, 200);
  }

  private async refreshMentionSuggestions(): Promise<void> {
    const input = this.layoutService.getElement<HTMLTextAreaElement>('prompt-input');
    const before = input.value.slice(0, input.selectionStart);
    const atIndex = before.lastIndexOf('@');
    if (atIndex < 0) {
      this.mentionSuggestions = [];
      return;
    }
    const query = before.slice(atIndex + 1).trim();
    this.mentionSuggestions = await this.workbenchService.fetchMentionSuggestions(query);
  }

  private visibleSlashCommands(text: string): typeof ComposerPart.SLASH_COMMANDS {
    const lower = text.toLowerCase();
    return ComposerPart.SLASH_COMMANDS.filter((c) => c.cmd.startsWith(lower) || lower === '/');
  }

  private updateSlashHints(input: HTMLTextAreaElement): void {
    const slashHints = this.layoutService.getElement<HTMLDivElement>('slash-hints');
    const text = input.value;
    if (!text.startsWith('/') || text.includes(' ') || text.includes('\n')) {
      slashHints.hidden = true;
      return;
    }
    this.slashSelectedIndex = 0;
    this.renderSlashHints(input, slashHints);
  }

  private renderSlashHints(input: HTMLTextAreaElement, container: HTMLDivElement): void {
    const cmds = this.visibleSlashCommands(input.value);
    if (cmds.length === 0) {
      container.hidden = true;
      return;
    }
    container.hidden = false;
    container.innerHTML = cmds
      .map(
        (c, i) => `
          <button class="slash-hints__item${i === this.slashSelectedIndex ? ' is-selected' : ''}" data-slash-cmd="${c.cmd}">
            <span class="slash-hints__cmd">${c.cmd}</span>
            <span class="slash-hints__desc">${c.desc}</span>
          </button>`,
      )
      .join('');
    container.querySelectorAll<HTMLButtonElement>('[data-slash-cmd]').forEach((btn, i) => {
      btn.addEventListener('mouseenter', () => {
        this.slashSelectedIndex = i;
        this.renderSlashHints(input, container);
      });
      btn.addEventListener('click', () => {
        input.value = btn.dataset.slashCmd + ' ';
        this.workbenchService.setInputValue(input.value);
        container.hidden = true;
        input.focus();
      });
    });
  }
}
