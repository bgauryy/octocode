// `schema [command] [recipe|operation]`: the commands, recipes and plan contract, without connecting.
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import {
  checks,
  helpers,
  rawCommands,
  launchBooleans,
  launchValues,
} from './cli-catalog.mjs';
import {
  chromeExtractionFields,
  chromePlanOperations,
  chromePlanExamples,
  chromeInputExamples,
  chromeLaunchGuide,
  chromeEmulationGuide,
  chromePlanOperationGuides,
  chromeRecipeGuides,
} from './chrome-contract.mjs';
import { fail, isOwnKey } from './cli-options.mjs';

/** The part of the plan executor the schema reads. */
type PlanContract = { DOM_ACTIONS: readonly string[] };

const SCHEMA_HELP =
  'Usage: octocode-cdp schema [command] [recipe|operation]\nExamples: schema run extract; schema check page-snapshot. No Chrome connection is made.';
const PLAN_COMMANDS = ['run', 'cdp', 'protocol'];

const EXTRACTION = {
  fields: chromeExtractionFields,
  defaults: ['text', 'href'],
  name: 'aria-label or name attribute; not computed accessible name',
};

// One line per operation for the overview; the field guide is one call away (schema run <op>).
const PLAN_PURPOSE: Record<string, string> = {
  goto: 'navigate the tab',
  history: 'back or forward in tab history',
  reload: 'reload the tab',
  act: 'DOM action on a target: ref, selector or role + name',
  wait: 'wait until a target or the after condition is visible',
  extract: 'read rows (text, href ...) from matching elements',
  evaluate: 'run a page expression and return its value',
  cdp: 'raw CDP method; sensitive ones need acknowledge',
  protocol: "read Chrome's live protocol schema",
  listen: 'subscribe to a CDP event under an id',
  waitEvent: 'wait for matches of a listen id',
  readStream: 'read an IO stream handle',
  snapshot: 'page refs (e1, e2 ...) for later act steps',
  screenshot: 'capture a screenshot artifact',
};

// Step fields every operation that accepts them shares: described once, named per operation.
const SHARED_FIELDS = ['after', 'frame', 'session', 'timeoutMs', 'optional'];
const SHARED_TEXT: Record<string, string> = {
  timeoutMs: 'Positive whole-step deadline',
  optional:
    'Boolean; a failure is recorded in failedOptionalSteps and the plan continues',
  after: 'Visible selector, text and/or exact url',
  frame: 'id, url substring or selector (supported ops only)',
  session:
    'Flattened session id or an earlier result/event reference; mutually exclusive with frame',
};

// Plan guidance that is the same for every operation, in output order.
const PLAN_OPTIONS = {
  steps:
    'Nonempty array, runs in order and stops on the first failing step that is not optional',
  waitMs: 'Positive step deadline in ms (default 8000)',
  commandMs: 'Positive request deadline, capped by remaining step time',
  settleMs: 'Nonnegative delay; prefer content conditions',
  traceEvents: 'Boolean, record event trust without typed characters',
  observe: {
    network: 'Boolean, subscribe before steps',
    afterMs: 'Nonnegative additional observation window',
  },
};
const PLAN_REFERENCE = {
  references: {
    $step:
      'Earlier CDP step number (1-based), pointer: JSON pointer into the step result itself, not wrapped in /result',
    $event: 'Earlier listener id, pointer: JSON pointer',
  },
  targets: 'ref, selector or exact role + name; ambiguity fails',
  validation:
    'run --plan file|- --dry-run validates with the executor before connecting',
};

/** A guide without the shared step fields: their names only, each described once under `shared`. */
function operationGuide(name: string) {
  const guide = chromePlanOperationGuides[name];
  const fields = Object.fromEntries(
    Object.entries(guide.fields).filter(([key]) => !SHARED_FIELDS.includes(key))
  );
  return {
    required: guide.required,
    fields,
    shared: SHARED_FIELDS.filter(key => Object.hasOwn(guide.fields, key)),
  };
}

/** The overview (one line per operation) or one operation's guide; schemaOutput rejects an unknown operation first. */
function planSchema(
  operation: string | undefined,
  { DOM_ACTIONS }: PlanContract
) {
  if (isOwnKey(chromePlanOperations, operation))
    return {
      operations: { [operation]: PLAN_PURPOSE[operation] },
      guides: { [operation]: operationGuide(operation) },
      ...(operation === 'act' ? { actions: DOM_ACTIONS } : {}),
      ...(operation === 'extract' ? { extraction: EXTRACTION } : {}),
      shared: Object.fromEntries(
        operationGuide(operation).shared.map(key => [key, SHARED_TEXT[key]])
      ),
      examples: [chromePlanExamples[operation]],
    };
  return {
    operations: Object.fromEntries(
      Object.keys(chromePlanOperations).map(name => {
        const required = chromePlanOperationGuides[name].required;
        return [
          name,
          `${required.length ? required.join(', ') : '(none)'} - ${PLAN_PURPOSE[name]}`,
        ];
      })
    ),
    shared: {
      ...SHARED_TEXT,
      note: 'each operation lists which of these it accepts; schema run <op> gives its fields, types and an example',
    },
    options: PLAN_OPTIONS,
    ...PLAN_REFERENCE,
    example: {
      steps: [chromePlanExamples.act, chromePlanExamples.snapshot],
    },
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
  // `launch` is an object the contract validates at call time; its fields come from that same schema.
  if (name === 'open')
    return {
      options: [...launchValues, ...launchBooleans],
      launch: chromeLaunchGuide,
      emulation: chromeEmulationGuide,
    };
  if (name === 'cleanup') return { options: ['--port', '--dry-run'] };
  return {};
}

function helperSchema(name: string | undefined, cliPath: string) {
  if (name === undefined || !helpers[name]) return {};
  // The helper's own usage text, options included.
  const run = spawnSync(
    process.execPath,
    [join(dirname(cliPath), helpers[name][0]), '--help'],
    { encoding: 'utf8', timeout: 10000 }
  );
  const usage = run.stdout.trim() || run.stderr.trim() || undefined;
  return {
    usage,
    help: { command: process.execPath, args: [cliPath, name, '--help'] },
    component: helpers[name][0],
  };
}

type SchemaTarget = {
  name?: string;
  recipe?: string;
  operation?: string;
};

/** Commands that take recipe options, and the recipe whose fields those options are. */
const OPTION_RECIPES: Record<string, string> = {
  snapshot: 'page-snapshot',
  screenshot: 'page-screenshot',
};

// schemaTarget rejects an unknown command or recipe first, so a named one is always a key.
function schemaDocument(
  { name, recipe, operation }: SchemaTarget,
  plan: PlanContract,
  cliPath: string
) {
  return {
    commands: isOwnKey(rawCommands, name)
      ? { [name]: rawCommands[name] }
      : rawCommands,
    inputExamples: inputExamples(name),
    ...commandOptions(name),
    ...helperSchema(name, cliPath),
    ...(isOwnKey(OPTION_RECIPES, name)
      ? { options: chromeRecipeGuides[OPTION_RECIPES[name]] }
      : {}),
    ...(name === 'check' || !name
      ? {
          checks: isOwnKey(checks, recipe)
            ? { [recipe]: { ...checks[recipe], ...chromeRecipeGuides[recipe] } }
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
  const operation = name === 'run' ? detail : undefined;
  if (name && !Object.hasOwn(rawCommands, name))
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
