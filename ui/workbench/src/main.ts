import { DisposableStore } from './base/common/lifecycle';
import { InstantiationService } from './platform/instantiation/common/instantiationService';
import { ServiceCollection } from './platform/instantiation/common/serviceCollection';
import { ActivitybarPart } from './workbench/parts/activitybarPart';
import { ComposerPart } from './workbench/parts/composerPart';
import { EditorPart } from './workbench/parts/editorPart';
import { QuickOpenPart } from './workbench/parts/quickOpenPart';
import { SidebarPart } from './workbench/parts/sidebarPart';
import { StatusbarPart } from './workbench/parts/statusbarPart';
import { CommandService } from './workbench/services/commandService';
import { LayoutService } from './workbench/services/layoutService';
import { NotificationService } from './workbench/services/notificationService';
import { ICommandService, ILayoutService, INotificationService, IWorkbenchService } from './workbench/services/serviceIds';
import { WorkbenchService } from './workbench/services/workbenchService';

declare global {
  interface Window {
    electronAPI?: {
      getPlatform?: () => Promise<string>;
    };
  }
}

const root = document.getElementById('app');

if (!root) {
  throw new Error('Workbench root element is missing');
}

const services = new ServiceCollection();
const instantiation = new InstantiationService(services);
const disposables = new DisposableStore();

const layoutService = services.set(ILayoutService, new LayoutService(root));
layoutService.renderShell();

void window.electronAPI
  ?.getPlatform?.()
  .then((platform) => {
    document.documentElement.dataset.platform = platform;
  })
  .catch((error) => {
    console.warn('Failed to detect platform for titlebar layout', error);
  });

const commandService = services.set(ICommandService, new CommandService(instantiation));
const notificationService = services.set(INotificationService, instantiation.createInstance(NotificationService));
const workbenchService = services.set(IWorkbenchService, instantiation.createInstance(WorkbenchService));
commandService.registerCoreCommands();

disposables.add(instantiation.createInstance(ActivitybarPart));
disposables.add(instantiation.createInstance(SidebarPart));
disposables.add(instantiation.createInstance(EditorPart));
disposables.add(instantiation.createInstance(ComposerPart));
disposables.add(instantiation.createInstance(StatusbarPart));
disposables.add(instantiation.createInstance(QuickOpenPart));

layoutService.getRoot().addEventListener('click', (event) => {
  const command = (event.target as HTMLElement).closest<HTMLElement>('[data-command]')?.dataset.command;
  if (!command) {
    return;
  }
  void commandService.executeCommand(command);
});

void workbenchService.start();

window.addEventListener('beforeunload', () => {
  disposables.dispose();
});
