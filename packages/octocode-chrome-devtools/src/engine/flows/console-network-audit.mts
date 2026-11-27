import type { FlowDefinition } from './flow-types.mjs';
const flow: FlowDefinition = {
  name: 'console-network-audit',
  area: 'audits',
  description:
    'Load a page and record network, console and exceptions. Then save a snapshot and a screenshot.',
  params: {
    url: { type: 'string', required: true, description: 'Page to audit' },
  },
  plan: {
    observe: { network: true },
    steps: [
      { op: 'cdp', method: 'Runtime.enable' },
      { op: 'listen', id: 'console', event: 'Runtime.consoleAPICalled' },
      { op: 'listen', id: 'exceptions', event: 'Runtime.exceptionThrown' },
      { op: 'goto', url: '{{url}}' },
      { op: 'snapshot' },
      { op: 'screenshot' },
    ],
  },
  expects: [
    'The network artifact lists every request seen during the load',
    'Console calls and exceptions are saved as event streams',
    'A snapshot and a screenshot are saved',
  ],
};
export default flow;
