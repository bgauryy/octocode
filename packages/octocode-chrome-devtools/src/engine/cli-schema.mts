// `schema [command] [recipe|operation]`: the commands, recipes and plan contract, without connecting.
import { checks, helpers, launchValues } from './cli-catalog.mjs';
import {
  chromeCommands,
  chromeExtractionFields,
  chromePlanOperations,
  chromePlanExamples,
  chromeInputExamples,
} from './chrome-contract.mjs';
import { fail, isOwnKey } from './cli-options.mjs';

/** The part of the plan executor the schema reads. */
type PlanContract = { DOM_ACTIONS: readonly string[] };

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

// schemaOutput rejects an unknown operation first, so a named one is always a key.
function planSchema(
  operation: string | undefined,
  { DOM_ACTIONS }: PlanContract
) {
  const known = isOwnKey(chromePlanOperations, operation)
    ? operation
    : undefined;
  return {
    operations: known
      ? { [known]: chromePlanOperations[known] }
      : chromePlanOperations,
    ...(!operation || operation === 'act' ? { actions: DOM_ACTIONS } : {}),
    ...(!operation || operation === 'extract'
      ? { extraction: EXTRACTION }
      : {}),
    ...PLAN_SCOPES,
    examples: known
      ? [chromePlanExamples[known]]
      : Object.values(chromePlanExamples),
    ...PLAN_REFERENCE,
  };
}

function inputExamples(name: string | undefined) {
  if (!name) return chromeInputExamples;
  return isOwnKey(chromeInputExamples, name)
    ? { [name]: chromeInputExamples[name] }
    : undefined;
}

// Raw connection flags and plan inputs live in each command's --help;
// typed callers follow inputExamples and the tool input schema.
function commandOptions(name: string | undefined) {
  if (name === 'open')
    return { options: [...launchValues, '--headless', '--dry-run'] };
  if (name === 'cleanup') return { options: ['--port', '--dry-run'] };
  return {};
}

function helperSchema(name: string | undefined, cliPath: string) {
  if (name === undefined || !helpers[name]) return {};
  return {
    help: { command: process.execPath, args: [cliPath, name, '--help'] },
    component: helpers[name][0],
  };
}

type SchemaTarget = {
  name?: string;
  recipe?: string;
  operation?: string;
};

// schemaTarget rejects an unknown command or recipe first, so a named one is always a key.
function schemaDocument(
  { name, recipe, operation }: SchemaTarget,
  plan: PlanContract,
  cliPath: string
) {
  return {
    commands: isOwnKey(chromeCommands, name)
      ? { [name]: chromeCommands[name] }
      : chromeCommands,
    inputExamples: inputExamples(name),
    ...commandOptions(name),
    ...helperSchema(name, cliPath),
    ...(name === 'check' || !name
      ? {
          checks: isOwnKey(checks, recipe)
            ? { [recipe]: checks[recipe] }
            : checks,
        }
      : {}),
    ...(!name || PLAN_COMMANDS.includes(name)
      ? { plan: planSchema(operation, plan) }
      : {}),
  };
}

function schemaTarget(args: string[]) {
  if (args.length > 2) fail('Usage: schema [command] [recipe|operation]');
  const [name, detail] = args;
  const recipe = name === 'check' ? detail : undefined;
  const operation = ['run', 'step'].includes(name) ? detail : undefined;
  if (name && !Object.hasOwn(chromeCommands, name))
    fail(`Unknown command ${name}`);
  if (recipe && (name !== 'check' || !Object.hasOwn(checks, recipe)))
    fail(`Unknown recipe ${recipe}`);
  return { name, detail, recipe, operation };
}

/** The schema command's output text; invalid names fail. */
export async function schemaOutput(
  args: string[],
  planContract: () => Promise<PlanContract>,
  cliPath: string
) {
  if (args.includes('--help') || args.includes('-h')) return SCHEMA_HELP;
  const target = schemaTarget(args);
  const { detail, recipe, operation } = target;
  const plan = await planContract();
  if (detail && !recipe && !operation)
    fail('Detail requires check recipe or run/step operation');
  if (operation && !Object.hasOwn(chromePlanOperations, operation))
    fail(
      `Unknown operation ${operation}; supported operations: ${Object.keys(chromePlanOperations).join(', ')}`
    );
  return JSON.stringify(schemaDocument(target, plan, cliPath), null, 2);
}
