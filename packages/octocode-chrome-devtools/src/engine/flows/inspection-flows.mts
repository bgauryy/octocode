// Inspection flows: read DOM, style, accessibility, performance, certificate and listener data through chained CDP results.
import type { FlowDefinition } from './flow-types.mjs';
import { FRAGMENT, ref } from './fragment.mjs';

const cssStyles: FlowDefinition = {
  name: 'css-styles',
  area: 'inspection',
  description: 'Computed and matched CSS of an element.',
  params: {
    selector: {
      type: 'string',
      required: true,
      description: 'CSS selector of the element',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'DOM.enable' },
      { op: 'cdp', method: 'CSS.enable' },
      { op: 'cdp', method: 'DOM.getDocument', params: { depth: 0 } },
      {
        op: 'cdp',
        method: 'DOM.querySelector',
        params: { nodeId: ref(3, '/root/nodeId'), selector: '{{selector}}' },
      },
      {
        op: 'cdp',
        method: 'CSS.getComputedStyleForNode',
        params: { nodeId: ref(4, '/nodeId') },
        save: 'css-styles.json',
      },
      {
        op: 'cdp',
        method: 'CSS.getMatchedStylesForNode',
        params: { nodeId: ref(4, '/nodeId') },
        save: 'css-matched.json',
      },
    ],
  },
  expects: [
    'saved/css-styles.json lists computedStyle {name, value}; saved/css-matched.json holds the matched rules',
    'A selector that matches nothing gives nodeId 0 and the style steps fail',
    FRAGMENT,
  ],
};

const accessibilityTree: FlowDefinition = {
  name: 'accessibility-tree',
  area: 'inspection',
  description: 'The full accessibility tree of the page.',
  params: {},
  plan: {
    steps: [
      { op: 'cdp', method: 'Accessibility.enable' },
      {
        op: 'cdp',
        method: 'Accessibility.getFullAXTree',
        save: 'ax-tree.json',
      },
    ],
  },
  expects: [
    'saved/ax-tree.json lists every accessibility node with role and name',
    FRAGMENT,
  ],
};

const pageMetrics: FlowDefinition = {
  name: 'page-metrics',
  area: 'inspection',
  description:
    'Performance metrics and DOM object counters. Metrics cover JS heap, layout counts, timestamps.',
  params: {},
  plan: {
    steps: [
      { op: 'cdp', method: 'Performance.enable' },
      {
        op: 'cdp',
        method: 'Performance.getMetrics',
        save: 'metrics.json',
      },
      { op: 'cdp', method: 'Memory.getDOMCounters' },
    ],
  },
  expects: [
    'saved/metrics.json holds metrics [{name, value}] including JSHeapUsedSize; cdp-3.json holds documents, nodes and jsEventListeners',
    FRAGMENT,
  ],
};

const securityInfo: FlowDefinition = {
  name: 'security-info',
  area: 'inspection',
  description: 'TLS certificate details of an HTTPS origin.',
  params: {
    origin: {
      type: 'string',
      required: true,
      pattern: '^https://',
      description: 'HTTPS origin, e.g. https://example.com',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'Network.enable' },
      {
        op: 'cdp',
        method: 'Network.getCertificate',
        params: { origin: '{{origin}}' },
        save: 'security.json',
      },
    ],
  },
  expects: [
    'saved/security.json lists the DER-encoded certificate chain as tableNames',
    FRAGMENT,
  ],
};

const eventListeners: FlowDefinition = {
  name: 'event-listeners',
  area: 'inspection',
  description:
    'Event listeners of an element. Each has type, handler source location, capture.',
  params: {
    selector: {
      type: 'string',
      required: true,
      description: 'CSS selector of the element',
    },
  },
  plan: {
    steps: [
      { op: 'cdp', method: 'DOM.getDocument', params: { depth: 0 } },
      {
        op: 'cdp',
        method: 'DOM.querySelector',
        params: { nodeId: ref(1, '/root/nodeId'), selector: '{{selector}}' },
      },
      {
        op: 'cdp',
        method: 'DOM.resolveNode',
        params: { nodeId: ref(2, '/nodeId') },
      },
      {
        op: 'cdp',
        method: 'DOMDebugger.getEventListeners',
        params: { objectId: ref(3, '/object/objectId') },
        save: 'event-listeners.json',
      },
    ],
  },
  expects: [
    'saved/event-listeners.json lists listeners [{type, useCapture, scriptId, lineNumber}]',
    FRAGMENT,
  ],
};

export const inspectionFlows: FlowDefinition[] = [
  cssStyles,
  accessibilityTree,
  pageMetrics,
  securityInfo,
  eventListeners,
];
