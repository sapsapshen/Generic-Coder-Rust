import { Disposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { ILayoutService } from './serviceIds';
import { LayoutService } from './layoutService';

export class NotificationService extends Disposable {
  private readonly layoutService: LayoutService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.layoutService = accessor.get(ILayoutService);
  }

  notify(message: string): void {
    const stack = this.layoutService.getElement<HTMLDivElement>('toast-stack');
    const item = document.createElement('div');
    item.className = 'toast';
    item.textContent = message;
    stack.appendChild(item);
    window.setTimeout(() => {
      item.remove();
    }, 3200);
  }
}
