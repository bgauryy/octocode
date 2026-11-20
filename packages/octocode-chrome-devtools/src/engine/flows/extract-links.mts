import type { FlowDefinition } from './flow-types.mjs';
const flow: FlowDefinition = {
  name: 'extract-links',
  description:
    'Extract text and href of every element matching a selector (default: all links).',
  params: {
    url: { type: 'string', description: 'Page to open first (optional)' },
    selector: {
      type: 'string',
      default: 'a',
      description: 'CSS selector of the links',
    },
  },
  plan: {
    steps: [
      { op: 'goto', url: '{{url}}' },
      {
        op: 'extract',
        selector: '{{selector}}',
        fields: ['text', 'href'],
      },
    ],
  },
  expects: ['The saved extract holds one row (text, href) per match'],
};
export default flow;
