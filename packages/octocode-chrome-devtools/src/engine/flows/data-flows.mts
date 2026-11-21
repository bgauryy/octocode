// Data flows: DOM snapshot, site storage dumps, service worker state, request mocking and blocking,
// system info and storage quota. Every method was checked against the installed Chrome's protocol.
import type { FlowDefinition, ParamDefinition } from './flow-types.mjs';
import { FRAGMENT, ref } from './fragment.mjs';

const origin = (what: string): ParamDefinition => ({
  type: 'string',
  required: true,
  pattern: '^https?://',
  description: `http(s) origin whose ${what}, e.g. https://example.com (the page must be at this origin)`,
});
const acknowledge = (what: string): ParamDefinition => ({
  type: 'boolean',
  required: true,
  description: `Must be true: confirms ${what}`,
});
const pageSize = (what: string): ParamDefinition => ({
  type: 'number',
  default: 100,
  min: 1,
  description: `How many ${what} to read (one page)`,
});
const ms = (default_: number, what: string): ParamDefinition => ({
  type: 'number',
  default: default_,
  min: 1,
  description: `How long to wait for ${what}, in milliseconds`,
});

const domSnapshot: FlowDefinition = {
  name: 'dom-snapshot',
  area: 'inspection',
  description: 'Capture the whole DOM with layout and paint order.',
  params: {
    computedStyle: {
      type: 'string',
      default: 'display',
      description:
        'One CSS property to capture for every node, e.g. display or color (add more by editing the spliced step)',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'DOMSnapshot.captureSnapshot',
        params: {
          computedStyles: ['{{computedStyle}}'],
          includePaintOrder: true,
          includeDOMRects: true,
        },
        save: 'dom-snapshot.json',
      },
    ],
  },
  expects: [
    'saved/dom-snapshot.json holds documents (nodes, layout, text boxes) and the shared strings table every row indexes into',
    FRAGMENT,
  ],
};

const indexedDbDump: FlowDefinition = {
  name: 'indexeddb-dump',
  area: 'inspection',
  sensitive: true,
  description:
    'Read an origin IndexedDB: stores and records. Sensitive: needs acknowledge true.',
  params: {
    origin: origin('IndexedDB to read'),
    pageSize: pageSize('records of the first object store'),
    acknowledge: acknowledge('reading stored site records is intended'),
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'IndexedDB.enable', acknowledge: '{{acknowledge}}' },
      {
        op: 'cdp',
        method: 'IndexedDB.requestDatabaseNames',
        acknowledge: '{{acknowledge}}',
        params: { securityOrigin: '{{origin}}' },
        save: 'idb-databases.json',
      },
      {
        op: 'cdp',
        method: 'IndexedDB.requestDatabase',
        acknowledge: '{{acknowledge}}',
        params: {
          securityOrigin: '{{origin}}',
          databaseName: ref(2, '/databaseNames/0'),
        },
        save: 'idb-database.json',
      },
      {
        op: 'cdp',
        method: 'IndexedDB.requestData',
        acknowledge: '{{acknowledge}}',
        params: {
          securityOrigin: '{{origin}}',
          databaseName: ref(2, '/databaseNames/0'),
          objectStoreName: ref(
            3,
            '/databaseWithObjectStores/objectStores/0/name'
          ),
          skipCount: 0,
          pageSize: '{{pageSize}}',
        },
        save: 'idb-data.json',
      },
    ],
  },
  expects: [
    'saved/idb-databases.json lists the database names, saved/idb-database.json its object stores and indexes, saved/idb-data.json one page of records (hasMore tells whether more exist); only the first database and first store are read, splice more steps for others',
    'An origin without a database fails the later steps; the values are raw site data and may hold secrets',
    FRAGMENT,
  ],
};

const cacheStorageDump: FlowDefinition = {
  name: 'cache-storage-dump',
  area: 'inspection',
  sensitive: true,
  description:
    'Read an origin CacheStorage: caches and entries. Sensitive: needs acknowledge true.',
  params: {
    origin: origin('caches to read'),
    pageSize: pageSize('entries of the first cache'),
    acknowledge: acknowledge('reading cached site responses is intended'),
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'CacheStorage.requestCacheNames',
        acknowledge: '{{acknowledge}}',
        params: { securityOrigin: '{{origin}}' },
        save: 'cache-names.json',
      },
      {
        op: 'cdp',
        method: 'CacheStorage.requestEntries',
        acknowledge: '{{acknowledge}}',
        params: {
          cacheId: ref(1, '/caches/0/cacheId'),
          skipCount: 0,
          pageSize: '{{pageSize}}',
        },
        save: 'cache-entries.json',
      },
    ],
  },
  expects: [
    'saved/cache-names.json lists the caches, saved/cache-entries.json one page of entries (request URL, status, headers) of the first cache; returnCount is the cache total',
    'An origin without a cache fails the second step',
    FRAGMENT,
  ],
};

const serviceWorkerState: FlowDefinition = {
  name: 'service-worker-state',
  area: 'inspection',
  description: 'Record service worker state in a window.',
  params: { ms: ms(3000, 'service worker events') },
  plan: {
    steps: [
      {
        op: 'listen',
        id: 'registrations',
        event: 'ServiceWorker.workerRegistrationUpdated',
      },
      {
        op: 'listen',
        id: 'versions',
        event: 'ServiceWorker.workerVersionUpdated',
      },
      {
        op: 'listen',
        id: 'errors',
        event: 'ServiceWorker.workerErrorReported',
      },
      { op: 'cdp', method: 'ServiceWorker.enable' },
      {
        op: 'waitEvent',
        listener: 'registrations',
        count: 1000000,
        timeoutMs: '{{ms}}',
        optional: true,
      },
    ],
  },
  expects: [
    'events-registrations.jsonl and events-versions.jsonl hold every registration and version the page has (Chrome replays existing ones right after enable); events-errors.jsonl holds worker errors',
    'The wait step times out by design (listed in failedOptionalSteps)',
    FRAGMENT,
  ],
};

const networkMock: FlowDefinition = {
  name: 'network-mock',
  area: 'emulation',
  sensitive: true,
  description:
    'Answer a matching request with a made-up reply. Sensitive: needs acknowledge true.',
  params: {
    pattern: {
      type: 'string',
      required: true,
      description:
        'URL pattern of the request, wildcards allowed, e.g. *://*/api/*',
    },
    status: {
      type: 'number',
      default: 200,
      min: 100,
      max: 599,
      description: 'HTTP status of the made-up reply',
    },
    body: {
      type: 'string',
      description: 'Reply body, base64-encoded (optional; empty when absent)',
    },
    trigger: {
      type: 'string',
      description:
        'JavaScript that makes the page send the request, run asynchronously (optional; else the page must send it by itself)',
    },
    ms: ms(5000, 'the matching request'),
    acknowledge: acknowledge(
      'answering a request with a made-up reply is intended'
    ),
  },
  plan: {
    steps: [
      { op: 'listen', id: 'paused', event: 'Fetch.requestPaused' },
      {
        op: 'cdp',
        method: 'Fetch.enable',
        params: { patterns: [{ urlPattern: '{{pattern}}' }] },
        acknowledge: '{{acknowledge}}',
      },
      // The trigger is the caller's own script, so it runs as the evaluate op.
      {
        op: 'evaluate',
        expression: 'setTimeout(function () { {{trigger}} }, 0)',
      },
      { op: 'waitEvent', listener: 'paused', timeoutMs: '{{ms}}' },
      {
        op: 'cdp',
        method: 'Fetch.fulfillRequest',
        acknowledge: '{{acknowledge}}',
        params: {
          requestId: { $event: 'paused', pointer: '/requestId' },
          responseCode: '{{status}}',
          body: '{{body}}',
        },
      },
      { op: 'cdp', method: 'Fetch.disable' },
    ],
  },
  expects: [
    'The first request matching the pattern is answered with the given status and body; events-paused.jsonl holds the paused request; no matching request within ms fails the wait',
    'Fetch is disabled again, so later requests are not paused',
    FRAGMENT,
  ],
};

const blockUrls: FlowDefinition = {
  name: 'block-urls',
  area: 'emulation',
  sensitive: true,
  description:
    'Block requests matching a URL pattern. Sensitive: needs acknowledge true.',
  params: {
    pattern: {
      type: 'string',
      required: true,
      description:
        'Absolute URLPattern, e.g. *://*:*/*.css (https://urlpattern.spec.whatwg.org/)',
    },
    acknowledge: acknowledge('blocking requests is intended'),
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'Network.enable' },
      {
        op: 'cdp',
        method: 'Network.setBlockedURLs',
        acknowledge: '{{acknowledge}}',
        params: { urlPatterns: [{ urlPattern: '{{pattern}}', block: true }] },
      },
    ],
  },
  expects: [
    'Requests matching the pattern fail with net::ERR_BLOCKED_BY_CLIENT for the rest of the run (Network.loadingFailed, blockedReason inspector)',
    FRAGMENT,
  ],
};

const systemInfo: FlowDefinition = {
  name: 'system-info',
  area: 'inspection',
  description: 'GPU, model and command line of the browser machine.',
  params: {},
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'SystemInfo.getInfo',
        scope: 'auto',
        save: 'system-info.json',
      },
    ],
  },
  expects: [
    'saved/system-info.json holds gpu (devices, feature status), modelName, modelVersion and commandLine',
    FRAGMENT,
  ],
};

const storageQuota: FlowDefinition = {
  name: 'storage-quota',
  area: 'inspection',
  description: 'Storage usage and quota of an origin.',
  params: {
    origin: {
      type: 'string',
      required: true,
      pattern: '^https?://',
      description: 'http(s) origin, e.g. https://example.com',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Storage.getUsageAndQuota',
        params: { origin: '{{origin}}' },
        save: 'storage-quota.json',
      },
    ],
  },
  expects: [
    'saved/storage-quota.json holds usage, quota, overrideActive and usageBreakdown [{storageType, usage}] in bytes',
    FRAGMENT,
  ],
};

export const dataFlows: FlowDefinition[] = [
  domSnapshot,
  indexedDbDump,
  cacheStorageDump,
  serviceWorkerState,
  networkMock,
  blockUrls,
  systemInfo,
  storageQuota,
];
