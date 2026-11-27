/** The command groups in catalog order; `schema` and the CLI list commands in this order. */
import { deliveryCommands } from './commands-delivery.ts';
import { leasesCommands } from './commands-leases.ts';
import { maintenanceCommands } from './commands-maintenance.ts';
import { messagingCommands } from './commands-messaging.ts';
import { presenceCommands } from './commands-presence.ts';
import { systemCommands } from './commands-system.ts';
import { workspaceCommands } from './commands-workspace.ts';
import type { CommandDefinition } from './registry.ts';

export const commandGroups: readonly (readonly CommandDefinition[])[] = [
  presenceCommands, messagingCommands, leasesCommands, systemCommands, deliveryCommands, workspaceCommands, maintenanceCommands,
];
