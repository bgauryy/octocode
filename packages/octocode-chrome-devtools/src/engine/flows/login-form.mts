import type { FlowDefinition } from './flow-types.mjs';
const flow: FlowDefinition = {
  name: 'login-form',
  area: 'input',
  description:
    'Sign in through a labelled username/password form and wait for a success text. Omit url to use the current page.',
  params: {
    url: { type: 'string', description: 'Page to open first (optional)' },
    userLabel: {
      type: 'string',
      required: true,
      description: 'Accessible name of the username textbox',
    },
    userValue: {
      type: 'string',
      required: true,
      description: 'Username to type',
    },
    passwordLabel: {
      type: 'string',
      required: true,
      description: 'Accessible name of the password textbox',
    },
    passwordValue: {
      type: 'string',
      required: true,
      secret: true,
      description: 'Password to type; never echoed',
    },
    submitName: {
      type: 'string',
      required: true,
      description: 'Accessible name of the submit button',
    },
    successText: {
      type: 'string',
      required: true,
      description: 'Text that appears once signed in',
    },
  },
  plan: {
    steps: [
      { op: 'goto', url: '{{url}}' },
      {
        op: 'act',
        action: 'fill',
        role: 'textbox',
        name: '{{userLabel}}',
        value: '{{userValue}}',
      },
      {
        op: 'act',
        action: 'fill',
        role: 'textbox',
        name: '{{passwordLabel}}',
        value: '{{passwordValue}}',
      },
      {
        op: 'act',
        action: 'click',
        role: 'button',
        name: '{{submitName}}',
        after: { text: '{{successText}}' },
      },
    ],
  },
  expects: [
    'The username and password fields accepted their values',
    'The submit button was clicked',
    'The success text is visible on the page',
  ],
};
export default flow;
