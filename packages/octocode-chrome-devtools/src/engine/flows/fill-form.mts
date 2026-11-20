import type { FlowDefinition } from './flow-types.mjs';
import type { ParamDefinition } from './flow-types.mjs';
const pair = (n: number): Record<string, ParamDefinition> => ({
  [`field${n}Label`]: {
    type: 'string',
    ...(n === 1 ? { required: true } : {}),
    description: `Accessible name of textbox ${n}${n === 1 ? '' : ` (give with field${n}Value)`}`,
  },
  [`field${n}Value`]: {
    type: 'string',
    ...(n === 1 ? { required: true } : {}),
    description: `Text for textbox ${n}`,
  },
});
const step = (n: number) => ({
  op: 'act',
  action: 'fill',
  role: 'textbox',
  name: `{{field${n}Label}}`,
  value: `{{field${n}Value}}`,
});
const flow: FlowDefinition = {
  name: 'fill-form',
  area: 'input',
  description:
    'Fill up to six labelled textboxes (fieldNLabel/fieldNValue pairs; a pair needs both), optionally click submit and wait for a text.',
  params: {
    url: { type: 'string', description: 'Page to open first (optional)' },
    ...[1, 2, 3, 4, 5, 6].reduce((all, n) => ({ ...all, ...pair(n) }), {}),
    submitName: {
      type: 'string',
      description: 'Accessible name of a button to click afterwards (optional)',
    },
    successText: {
      type: 'string',
      description: 'Text to wait for after submitting (optional)',
    },
  },
  plan: {
    steps: [
      { op: 'goto', url: '{{url}}' },
      ...[1, 2, 3, 4, 5, 6].map(step),
      { op: 'act', action: 'click', role: 'button', name: '{{submitName}}' },
      { op: 'wait', value: '{{successText}}' },
    ],
  },
  expects: [
    'Every given textbox accepted its value',
    'The submit button was clicked when submitName was given',
    'The success text is visible when successText was given',
  ],
};
export default flow;
