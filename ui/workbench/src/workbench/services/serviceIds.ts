import type { CommandService } from './commandService';
import { createServiceIdentifier } from '../../platform/instantiation/common/serviceIdentifier';
import type { LayoutService } from './layoutService';
import type { NotificationService } from './notificationService';
import type { WorkbenchService } from './workbenchService';

export const ICommandService = createServiceIdentifier<CommandService>('commandService');
export const ILayoutService = createServiceIdentifier<LayoutService>('layoutService');
export const INotificationService = createServiceIdentifier<NotificationService>('notificationService');
export const IWorkbenchService = createServiceIdentifier<WorkbenchService>('workbenchService');
