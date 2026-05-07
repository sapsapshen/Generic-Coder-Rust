import { Disposable, toDisposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { escapeHtml } from '../common/dom';
import { CommandService } from '../services/commandService';
import { LayoutService } from '../services/layoutService';
import { ICommandService, ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

export class QuickOpenPart extends Disposable {
  private currentQuery = '';
  private currentVisibility = false;

  private readonly layoutService: LayoutService;
  private readonly workbenchService: WorkbenchService;
  private readonly commandService: CommandService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.layoutService = accessor.get(ILayoutService);
    this.workbenchService = accessor.get(IWorkbenchService);
    this.commandService = accessor.get(ICommandService);
    this.bind();
    this._register(this.workbenchService.onDidChangeState(() => this.render()));
    this.render();
  }

  private bind(): void {
    const shell = this.layoutService.getElement<HTMLDivElement>('quick-open');
    const scrim = shell.querySelector<HTMLDivElement>('.quick-open__scrim');
    const input = this.layoutService.getElement<HTMLInputElement>('quick-open-input');
    const close = () => {
      this.hideShell(shell);
      this.workbenchService.setQuickOpenVisible(false);
    };

    input.addEventListener('input', () => {
      this.currentQuery = input.value;
      void this.renderResults();
    });
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        close();
        return;
      }
      if (event.key === 'Enter') {
        const first = this.layoutService.getElement<HTMLDivElement>('quick-open-results').querySelector<HTMLElement>('[data-quick-kind]');
        if (first) {
          event.preventDefault();
          void this.executeItem(first.dataset.quickKind || '', first.dataset.quickValue || '');
        }
      }
    });

    scrim?.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      close();
    });
    shell.addEventListener('pointerdown', (event) => {
      if (event.target === shell) {
        event.preventDefault();
        close();
      }
    });

    const keydownHandler = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
    };
    document.addEventListener('keydown', keydownHandler, true);
    this._register(toDisposable(() => document.removeEventListener('keydown', keydownHandler, true)));
  }

  render(): void {
    const visible = this.workbenchService.state.quickOpenVisible;
    const shell = this.layoutService.getElement<HTMLDivElement>('quick-open');
    shell.hidden = !visible;
    shell.classList.toggle('is-visible', visible);
    shell.setAttribute('aria-hidden', String(!visible));
    if (visible && !this.currentVisibility) {
      this.currentQuery = '';
      const input = this.layoutService.getElement<HTMLInputElement>('quick-open-input');
      input.value = '';
      void this.renderResults();
      input.focus();
    }
    this.currentVisibility = visible;
  }

  private hideShell(shell: HTMLElement): void {
    shell.hidden = true;
    shell.classList.remove('is-visible');
    shell.setAttribute('aria-hidden', 'true');
    this.currentVisibility = false;
  }

  async renderResults(): Promise<void> {
    const results = this.layoutService.getElement<HTMLDivElement>('quick-open-results');
    const query = this.currentQuery.toLowerCase();
    const commands = this.commandService
      .getCommands()
      .filter((command) => command.label.toLowerCase().includes(query))
      .map((command) => ({ kind: 'command' as const, label: command.label, value: command.id }));
    const files = await this.workbenchService.getQuickOpenFileResults(this.currentQuery);
    const rows = [...commands, ...files];
    results.innerHTML = rows
      .map(
        (row) => `
          <button class="quick-open__item" data-quick-kind="${row.kind}" data-quick-value="${escapeHtml(row.value)}">
            <i class="codicon codicon-${row.kind === 'file' ? 'file' : 'terminal-cmd'}"></i>
            <span>${escapeHtml(row.label)}</span>
          </button>`,
      )
      .join('');
    results.querySelectorAll<HTMLElement>('[data-quick-kind]').forEach((button) => {
      button.addEventListener('click', () => {
        void this.executeItem(button.dataset.quickKind || '', button.dataset.quickValue || '');
      });
    });
  }

  private async executeItem(kind: string, value: string): Promise<void> {
    this.workbenchService.setQuickOpenVisible(false);
    if (kind === 'command') {
      await this.commandService.executeCommand(value);
      return;
    }
    if (kind === 'file') {
      await this.workbenchService.openPreviewTab(value);
    }
  }
}
