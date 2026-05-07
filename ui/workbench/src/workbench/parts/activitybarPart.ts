import { Disposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { escapeHtml } from '../common/dom';
import type { ViewId } from '../common/state';
import { LayoutService } from '../services/layoutService';
import { ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

export class ActivitybarPart extends Disposable {
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
    const items: Array<{ id: ViewId; icon: string; label: string }> = [
      { id: 'chat', icon: 'comment-discussion', label: 'Assistant' },
      { id: 'explorer', icon: 'files', label: 'Explorer' },
      { id: 'scm', icon: 'source-control', label: 'Changes' },
      { id: 'workflow', icon: 'server-process', label: 'Workflow' },
      { id: 'extensions', icon: 'extensions', label: 'Skills' },
      { id: 'settings', icon: 'settings-gear', label: 'Settings' },
    ];
    const container = this.layoutService.getElement<HTMLDivElement>('activitybar');
    container.innerHTML = items
      .map(
        (item) => `
          <button class="activitybar__item${item.id === this.workbenchService.state.activeView ? ' is-active' : ''}" data-view="${item.id}" title="${escapeHtml(item.label)}">
            <i class="codicon codicon-${item.icon}"></i>
          </button>
        `,
      )
      .join('');
    container.querySelectorAll<HTMLElement>('[data-view]').forEach((button) => {
      button.addEventListener('click', () => {
        this.workbenchService.setActiveView(button.dataset.view as ViewId);
      });
    });
  }
}
