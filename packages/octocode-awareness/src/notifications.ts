/** Public compatibility barrel for notifications.ts. */
export { insertNotification } from './notifications-core.js';
export { getNotifications, resolveNotification } from './notifications-inbox.js';
export { agentSignal, pruneNotifications } from './notifications-signals.js';
