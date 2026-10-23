import { StringEnum } from '@earendil-works/pi-ai';
import { Type, type Static, type TUnsafe } from 'typebox';

/** The `browser` tool's parameters and actions. */
const ACTIONS = ['info', 'attach', 'navigate', 'snapshot', 'click', 'hover', 'drag', 'type', 'fill', 'press', 'wait', 'tabs', 'tab', 'evaluate', 'screenshot', 'console', 'dialog', 'upload', 'download', 'close'] as const;
/** Default and longest `wait`, in seconds. */
export const WAIT_DEFAULT_S = 10;
export const WAIT_MAX_S = 60;

// Typed with this package's typebox: pi-ai's StringEnum returns its own nested copy's type, which declarations cannot name.
const ACTION: TUnsafe<BrowserAction> = StringEnum(ACTIONS);
const FORMAT: TUnsafe<string> = StringEnum(['jpeg', 'png'], { description: 'screenshot: image format (default jpeg; png only when the user asks for it)' });

export const PARAMETERS = Type.Object({
  action: ACTION,
  url: Type.Optional(Type.String({ description: 'navigate: URL; waits until the page is quiet (up to 6s); tab: URL to open' })),
  port: Type.Optional(Type.Integer({ description: 'attach: DevTools port of an open browser' })),
  ref: Type.Optional(Type.Integer({ description: 'Element number from snapshot (open shadow DOM and same-origin frames included)' })),
  to: Type.Optional(Type.Integer({ description: 'drag: element number to drop on (or give text instead)' })),
  fields: Type.Optional(Type.Array(Type.Object({ ref: Type.Integer(), value: Type.String() }), { description: 'fill: fields to set' })),
  index: Type.Optional(Type.Integer({ description: 'tab: tab number from tabs' })),
  gone: Type.Optional(Type.Boolean({ description: 'wait: until text disappears' })),
  timeout: Type.Optional(Type.Number({ description: `wait (and navigate/snapshot with text): seconds (default ${WAIT_DEFAULT_S}, max ${WAIT_MAX_S})` })),
  text: Type.Optional(Type.String({ description: 'type: value to enter, replacing the current one. wait: text to wait for (gone: to disappear). navigate/snapshot: text to wait for before returning. drag: visible text of the drop zone. dialog: reply to a prompt().' })),
  format: Type.Optional(FORMAT),
  accept: Type.Optional(Type.Boolean({ description: 'dialog: accept (default) or false to dismiss' })),
  files: Type.Optional(Type.Array(Type.String(), { description: 'upload: file paths' })),
  submit: Type.Optional(Type.Boolean({ description: 'Press Enter after typing' })),
  key: Type.Optional(Type.String({ description: 'press: key or combo, e.g. Enter, Escape, Control+A, Shift+Tab' })),
  expression: Type.Optional(Type.String({ description: 'JavaScript expression for evaluate' })),
});
export type BrowserParams = Static<typeof PARAMETERS>;

export type BrowserAction = (typeof ACTIONS)[number];
/** Actions that manage the session itself rather than act on its page. */
export type SessionAction = 'close' | 'info' | 'attach';
export type PageActionName = Exclude<BrowserAction, SessionAction>;
