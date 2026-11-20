import type { FlowDefinition } from './flow-types.mjs';
const flow: FlowDefinition = {
  name: 'full-page-screenshot',
  area: 'inspection',
  description: 'Open a page (optional) and save a screenshot of it.',
  params: {
    url: { type: 'string', description: 'Page to open first (optional)' },
  },
  plan: {
    steps: [{ op: 'goto', url: '{{url}}' }, { op: 'screenshot' }],
  },
  expects: ['A screenshot artifact of the page is saved'],
};
export default flow;
