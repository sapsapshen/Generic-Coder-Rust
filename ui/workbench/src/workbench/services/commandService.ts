import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import type { ViewId } from '../common/state';
import { IWorkbenchService } from './serviceIds';

export interface CommandDescriptor {
  id: string;
  label: string;
  run: () => void | Promise<void>;
}

export class CommandService {
  private readonly commands = new Map<string, CommandDescriptor>();

  constructor(private readonly accessor: ServicesAccessor) {}

  registerCommand(command: CommandDescriptor): void {
    this.commands.set(command.id, command);
  }

  getCommands(): CommandDescriptor[] {
    return [...this.commands.values()];
  }

  async executeCommand(id: string): Promise<void> {
    const command = this.commands.get(id);
    if (!command) {
      throw new Error(`Unknown command: ${id}`);
    }
    await command.run();
  }

  registerCoreCommands(): void {
    const workbench = this.accessor.get(IWorkbenchService);
    const openView = (view: ViewId) => () => workbench.setActiveView(view);

    this.registerCommand({ id: 'workbench.action.quickOpen', label: 'Quick Open', run: () => workbench.setQuickOpenVisible(true) });
    this.registerCommand({ id: 'workbench.action.closeQuickOpen', label: 'Close Quick Open', run: () => workbench.setQuickOpenVisible(false) });
    this.registerCommand({ id: 'workbench.action.newChat', label: 'New Chat', run: () => workbench.sendPrompt('/new') });
    this.registerCommand({ id: 'workbench.action.stop', label: 'Stop', run: () => workbench.stopTask() });
    this.registerCommand({ id: 'workbench.action.refresh', label: 'Refresh', run: () => workbench.refreshAll() });
    this.registerCommand({ id: 'workbench.action.toggleSidebar', label: 'Toggle Sidebar', run: () => workbench.toggleSidebar() });
    this.registerCommand({ id: 'workbench.view.chat', label: 'Open Assistant View', run: openView('chat') });
    this.registerCommand({ id: 'workbench.view.explorer', label: 'Open Explorer View', run: openView('explorer') });
    this.registerCommand({ id: 'workbench.view.changes', label: 'Open Changes View', run: openView('scm') });
    this.registerCommand({ id: 'workbench.view.extensions', label: 'Open Skills View', run: openView('extensions') });
    this.registerCommand({ id: 'workbench.view.settings', label: 'Open Settings View', run: openView('settings') });
  }
}
