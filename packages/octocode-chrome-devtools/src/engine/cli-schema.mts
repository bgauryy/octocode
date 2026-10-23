// `schema [command] [recipe|operation]`: the commands, recipes and plan contract, without connecting.
import { checks, commands, helpers, launchValues } from './cli-catalog.mjs';
import {
  chromeExtractionFields,
  chromePlanExamples,
  chromeInputExamples,
} from './chrome-contract.mjs';
import { fail } from './cli-options.mjs';

const SCHEMA_HELP =
  'Usage: octocode-cdp schema [command] [recipe|operation]\nExamples: schema run extract; schema check page-snapshot. No Chrome connection is made.';
const PLAN_COMMANDS = ['run', 'step', 'cdp', 'protocol'];

const EXTRACTION = {
  fields: chromeExtractionFields,
  defaults: ['text', 'href'],
  name: 'aria-label or name attribute; not computed accessible name',
};

// Plan guidance that is the same for every operation, in output order.
const PLAN_SCOPES = {
  frameScopes: {
    selector: ['act', 'wait', 'extract', 'cdp'],
    isolatedTarget: ['act', 'wait', 'extract', 'cdp', 'listen'],
  },
  common: {
    timeoutMs: 'Positive whole-step deadline',
    after: 'Visible selector, text and/or exact url',
    frame: 'id, url substring or selector (supported ops only)',
    session:
      'Flattened session id or an earlier result/event reference; mutually exclusive with frame',
  },
  options: {
    steps: 'Nonempty array, runs in order and stops on first failure',
    waitMs: 'Positive step deadline in ms',
    commandMs: 'Positive request deadline, capped by remaining step time',
    settleMs: 'Nonnegative delay; prefer content conditions',
    traceEvents: 'Boolean, record event trust without typed characters',
    observe: {
      network: 'Boolean, subscribe before steps',
      afterMs: 'Nonnegative additional observation window',
    },
  },
  targets: 'ref, selector or exact role + name; ambiguity fails',
  conditions:
    'after is an object with nonempty selector, text and/or exact url',
  frameOperations: ['act', 'wait', 'extract', 'cdp', 'listen'],
  sessionOperations: [
    'goto',
    'act',
    'wait',
    'extract',
    'cdp',
    'listen',
    'readStream',
  ],
};
const PLAN_REFERENCE = {
  defaults: { waitMs: 8000, settleMs: 0, commandMs: 'waitMs' },
  references: {
    $step: 'Earlier CDP step number (1-based), pointer: JSON pointer',
    $event: 'Earlier listener id, pointer: JSON pointer',
  },
  example: {
    steps: [
      {
        op: 'cdp',
        method: 'Runtime.evaluate',
        params: { expression: 'document.title', returnByValue: true },
      },
    ],
  },
  validation:
    'run --plan file|- --dry-run validates with the executor before connecting',
};

function planSchema(operation, { planOperations, actions }) {
  return {
    operations: operation
      ? { [operation]: planOperations[operation] }
      : planOperations,
    ...(!operation || operation === 'act' ? { actions } : {}),
    ...(!operation || operation === 'extract'
      ? { extraction: EXTRACTION }
      : {}),
    ...PLAN_SCOPES,
    examples: operation
      ? [chromePlanExamples[operation]]
      : Object.values(chromePlanExamples),
    ...PLAN_REFERENCE,
  };
}

function inputExamples(name) {
  if (!name) return chromeInputExamples;
  return chromeInputExamples[name]
    ? { [name]: chromeInputExamples[name] }
    : undefined;
}

// Raw connection flags and plan inputs live in each command's --help;
// typed callers follow inputExamples and the tool input schema.
function commandOptions(name) {
  if (name === 'open')
    return { options: [...launchValues, '--headless', '--dry-run'] };
  if (name === 'cleanup') return { options: ['--port', '--dry-run'] };
  return {};
}

function helperSchema(name, cliPath) {
  if (!helpers[name]) return {};
  return {
    help: { command: process.execPath, args: [cliPath, name, '--help'] },
    component: helpers[name][0],
  };
}

function schemaDocument({ name, recipe, operation }, plan, cliPath) {
  return {
    commands: name ? { [name]: commands[name] } : commands,
    inputExamples: inputExamples(name),
    ...commandOptions(name),
    ...helperSchema(name, cliPath),
    ...(name === 'check' || !name
      ? { checks: recipe ? { [recipe]: checks[recipe] } : checks }
      : {}),
    ...(PLAN_COMMANDS.includes(name) || !name
      ? { plan: planSchema(operation, plan) }
      : {}),
  };
}

function schemaTarget(args) {
  if (args.length > 2) fail('Usage: schema [command] [recipe|operation]');
  const [name, detail] = args;
  const recipe = name === 'check' ? detail : undefined;
  const operation = ['run', 'step'].includes(name) ? detail : undefined;
  if (name && !Object.hasOwn(commands, name)) fail(`Unknown command ${name}`);
  if (recipe && (name !== 'check' || !Object.hasOwn(checks, recipe)))
    fail(`Unknown recipe ${recipe}`);
  return { name, detail, recipe, operation };
}

/** The schema command's output text; invalid names fail. */
export async function schemaOutput(args, planContract, cliPath) {
  if (args.includes('--help') || args.includes('-h')) return SCHEMA_HELP;
  const target = schemaTarget(args);
  const { detail, recipe, operation } = target;
  const plan = await planContract();
  if (detail && !recipe && !operation)
    fail('Detail requires check recipe or run/step operation');
  if (operation && !Object.hasOwn(plan.planOperations, operation))
    fail(
      `Unknown operation ${operation}; supported operations: ${Object.keys(plan.planOperations).join(', ')}`
    );
  return JSON.stringify(schemaDocument(target, plan, cliPath), null, 2);
}
