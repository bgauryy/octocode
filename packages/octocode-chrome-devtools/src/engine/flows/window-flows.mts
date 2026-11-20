// Permission and event-window flows: grant permissions, answer dialogs, add virtual authenticators, observe audits, animations, media and layers.
import type { FlowDefinition } from './flow-types.mjs';
import { FRAGMENT } from './fragment.mjs';

const ms = (what: string) => ({
  type: 'number' as const,
  default: 3000,
  min: 1,
  description: `How long to observe ${what}, in milliseconds`,
});
// An event window: the wait step ends by timeout (optional) because count is never reached.
const window = (listener: string) => ({
  op: 'waitEvent',
  listener,
  count: 1000000,
  timeoutMs: '{{ms}}',
  optional: true,
});
const WINDOW_NOTE =
  'The wait step times out by design (listed in failedOptionalSteps); every event in the window is saved in events-<id>.jsonl';

const grantPermission: FlowDefinition = {
  name: 'grant-permission',
  area: 'permissions',
  sensitive: true,
  description:
    'Grant one browser permission to one origin without a prompt. Sensitive: needs acknowledge true.',
  params: {
    permission: {
      type: 'string',
      required: true,
      enum: [
        'geolocation',
        'notifications',
        'clipboardReadWrite',
        'clipboardSanitizedWrite',
        'midi',
        'sensors',
      ],
      description:
        'Protocol permission name (clipboardReadWrite = clipboard read, clipboardSanitizedWrite = clipboard write)',
    },
    origin: {
      type: 'string',
      required: true,
      pattern: '^https?://',
      description: 'http(s) origin, e.g. https://example.com',
    },
    acknowledge: {
      type: 'boolean',
      required: true,
      description: 'Must be true: confirms the grant is intended',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Browser.grantPermissions',
        acknowledge: '{{acknowledge}}',
        params: { permissions: ['{{permission}}'], origin: '{{origin}}' },
      },
    ],
  },
  expects: [
    'navigator.permissions.query reports granted for that origin',
    FRAGMENT,
  ],
};

const handleDialog: FlowDefinition = {
  name: 'handle-dialog',
  area: 'input',
  description:
    'Wait for a JavaScript dialog (alert, confirm, prompt, beforeunload) and accept or dismiss it. Optional `trigger` is JavaScript run after the listener exists that opens the dialog.',
  params: {
    accept: {
      type: 'boolean',
      default: true,
      description: 'true accepts the dialog, false dismisses it',
    },
    promptText: {
      type: 'string',
      description: 'Text entered into a prompt (optional)',
    },
    trigger: {
      type: 'string',
      description:
        'JavaScript that opens the dialog; run asynchronously so the call returns (optional)',
    },
    ms: {
      type: 'number',
      default: 5000,
      min: 1,
      description: 'How long to wait for the dialog, in milliseconds',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'Page.enable' },
      { op: 'listen', id: 'dialog', event: 'Page.javascriptDialogOpening' },
      {
        op: 'cdp',
        method: 'Runtime.evaluate',
        params: { expression: 'setTimeout(function () { {{trigger}} }, 0)' },
      },
      { op: 'waitEvent', listener: 'dialog', timeoutMs: '{{ms}}' },
      {
        op: 'cdp',
        method: 'Page.handleJavaScriptDialog',
        params: { accept: '{{accept}}', promptText: '{{promptText}}' },
      },
    ],
  },
  expects: [
    'The dialog is closed and the page script continues with the chosen answer; no dialog within ms fails the wait',
    FRAGMENT,
  ],
};

const auditsWindow: FlowDefinition = {
  name: 'audits-window',
  area: 'audits',
  description:
    'Collect DevTools Issues (cookies, mixed content, CORS, deprecations) reported during a time window.',
  params: { ms: ms('issues') },
  plan: {
    steps: [
      { op: 'cdp', method: 'Audits.enable' },
      { op: 'listen', id: 'issues', event: 'Audits.issueAdded' },
      window('issues'),
    ],
  },
  expects: [WINDOW_NOTE, FRAGMENT],
};

const webauthn: FlowDefinition = {
  name: 'webauthn-authenticator',
  area: 'permissions',
  sensitive: true,
  description:
    'Attach a virtual WebAuthn authenticator so pages can create and use passkeys. Sensitive: needs acknowledge true.',
  params: {
    protocol: {
      type: 'string',
      default: 'ctap2',
      enum: ['u2f', 'ctap2'],
      description: 'Authenticator protocol',
    },
    transport: {
      type: 'string',
      default: 'internal',
      enum: ['usb', 'nfc', 'ble', 'cable', 'hybrid', 'smart-card', 'internal'],
      description: 'Authenticator transport',
    },
    acknowledge: {
      type: 'boolean',
      required: true,
      description: 'Must be true: confirms a virtual authenticator is intended',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'WebAuthn.enable', acknowledge: '{{acknowledge}}' },
      {
        op: 'cdp',
        method: 'WebAuthn.addVirtualAuthenticator',
        acknowledge: '{{acknowledge}}',
        params: {
          options: { protocol: '{{protocol}}', transport: '{{transport}}' },
        },
      },
    ],
  },
  expects: [
    'cdp-2.json returns the authenticatorId; navigator.credentials works against it',
    FRAGMENT,
  ],
};

const animationTimeline: FlowDefinition = {
  name: 'animation-timeline',
  area: 'audits',
  description:
    'Record animation events (started, updated, canceled) during a time window.',
  params: { ms: ms('animations') },
  plan: {
    steps: [
      { op: 'cdp', method: 'Animation.enable' },
      { op: 'listen', id: 'started', event: 'Animation.animationStarted' },
      { op: 'listen', id: 'updated', event: 'Animation.animationUpdated' },
      { op: 'listen', id: 'canceled', event: 'Animation.animationCanceled' },
      window('started'),
    ],
  },
  expects: [WINDOW_NOTE, FRAGMENT],
};

const mediaEvents: FlowDefinition = {
  name: 'media-events',
  area: 'audits',
  description:
    'Record media player events and property changes during a time window.',
  params: { ms: ms('media players') },
  plan: {
    steps: [
      { op: 'cdp', method: 'Media.enable' },
      { op: 'listen', id: 'events', event: 'Media.playerEventsAdded' },
      {
        op: 'listen',
        id: 'properties',
        event: 'Media.playerPropertiesChanged',
      },
      window('events'),
    ],
  },
  expects: [WINDOW_NOTE, FRAGMENT],
};

const layerTree: FlowDefinition = {
  name: 'layer-tree',
  area: 'inspection',
  description:
    'The compositing layer tree: a forced frame makes LayerTree report the current layers as an event.',
  params: {
    ms: {
      type: 'number',
      default: 3000,
      min: 1,
      description: 'How long to wait for the layer tree event, in milliseconds',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'DOM.enable' },
      { op: 'listen', id: 'layers', event: 'LayerTree.layerTreeDidChange' },
      { op: 'cdp', method: 'LayerTree.enable' },
      // A headless page paints no frame by itself, so no layer event comes until one is forced.
      {
        op: 'cdp',
        method: 'Page.captureScreenshot',
        params: { format: 'jpeg', quality: 10 },
      },
      { op: 'waitEvent', listener: 'layers', timeoutMs: '{{ms}}' },
    ],
  },
  expects: [
    'events-layers.jsonl holds the layers (id, bounds, drawsContent) of the page',
    FRAGMENT,
  ],
};

export const windowFlows: FlowDefinition[] = [
  grantPermission,
  handleDialog,
  auditsWindow,
  webauthn,
  animationTimeline,
  mediaEvents,
  layerTree,
];
