import type { FlowDefinition } from './flow-types.mjs';
const flow: FlowDefinition = {
  name: 'paginate-list',
  area: 'input',
  description:
    'Extract a list, then up to maxPages-1 times click the next control (optional, so the last page ends cleanly) and extract again. After the last page the remaining clicks fail as skipped and the final page is extracted again.',
  params: {
    url: { type: 'string', description: 'Page to open first (optional)' },
    itemSelector: {
      type: 'string',
      required: true,
      description: 'CSS selector of the list items',
    },
    nextName: {
      type: 'string',
      required: true,
      description: 'Accessible name of the next-page control',
    },
    nextRole: {
      type: 'string',
      default: 'link',
      description: 'Role of the next-page control: link or button',
    },
    maxPages: {
      type: 'number',
      default: 5,
      description: 'Pages to extract at most (at least 1)',
    },
  },
  plan: {
    steps: [
      { op: 'goto', url: '{{url}}' },
      {
        op: 'extract',
        selector: '{{itemSelector}}',
        fields: ['text', 'href'],
      },
    ],
  },
  repeat: {
    param: 'maxPages',
    offset: -1,
    steps: [
      {
        op: 'act',
        action: 'click',
        role: '{{nextRole}}',
        name: '{{nextName}}',
        optional: true,
      },
      {
        op: 'extract',
        selector: '{{itemSelector}}',
        fields: ['text', 'href'],
      },
    ],
  },
  expects: [
    'One saved extract per visited page, at most maxPages',
    'Skipped optional clicks mean the last page was reached',
  ],
};
export default flow;
