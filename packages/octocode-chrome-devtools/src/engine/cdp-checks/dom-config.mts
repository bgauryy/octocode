// DOM operation inputs: run options (from the caller or DOM_* env) and validated steps.
import { errorMessage } from '../error-message.mjs';
import { buildKeyPressEvents } from '../human-input.mjs';
import type { DomConfig, DomOptions, DomStep, RawStep } from './dom-types.mjs';

/** Every DOM step action, in documented order. */
export const DOM_ACTIONS = [
  'inspect',
  'click',
  'dblclick',
  'fill',
  'type',
  'press',
  'hover',
  'select',
  'check',
  'uncheck',
  'focus',
  'scroll',
  'upload',
  'drag',
  'wait',
];

/** The operation an acting step reports once it succeeded. */
export const DONE: Record<string, string> = {
  click: 'clicked',
  dblclick: 'double-clicked',
  fill: 'filled',
  type: 'typed',
  press: 'pressed',
  hover: 'hovered',
  select: 'selected',
  check: 'checked',
  uncheck: 'unchecked',
  focus: 'focused',
  scroll: 'scrolled',
  upload: 'uploaded',
  drag: 'dragged',
  wait: 'waited',
};

const DEFAULT_SELECTOR =
  'button, [role="button"], input, textarea, select, a[href]';

function integerOption(value: unknown, min: number, name: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < min)
    throw new Error(`Invalid ${name}`);
  return parsed;
}

function timing(options: DomOptions) {
  const env = process.env;
  return {
    stabilityMs: integerOption(
      options.stabilityMs ?? env.DOM_STABILITY_MS ?? '150',
      0,
      'stabilityMs'
    ),
    settleMs: integerOption(
      options.settleMs ?? env.DOM_SETTLE_MS ?? '500',
      0,
      'settleMs'
    ),
    waitMs: integerOption(
      options.waitMs ?? env.DOM_WAIT_MS ?? '8000',
      1,
      'waitMs'
    ),
  };
}

/** Run-wide settings; an option wins over its DOM_* environment variable. */
export function readDomConfig(options: DomOptions): DomConfig {
  const env = process.env;
  const mode = (options.input ?? env.DOM_INPUT) === 'js' ? 'js' : 'trusted';
  return {
    mode,
    ...timing(options),
    waitText: options.waitText ?? env.DOM_WAIT_TEXT ?? '',
    acceptDialog: (options.dialog ?? env.DOM_DIALOG) === 'accept',
    diff: options.diff ?? env.DOM_DIFF !== '0',
    traceEvents: options.traceEvents ?? env.DOM_TRACE_EVENTS === '1',
    literalWaitText: options.literalWaitText,
  };
}

function parseEnvSteps(text: string): RawStep[] {
  let steps: unknown;
  try {
    steps = JSON.parse(text);
  } catch (error) {
    throw new Error(`DOM_STEPS is not valid JSON: ${errorMessage(error)}`, {
      cause: error,
    });
  }
  if (!Array.isArray(steps) || !steps.length)
    throw new Error('DOM_STEPS must be a non-empty JSON array of steps');
  return steps;
}

function envStep(): RawStep {
  const env = process.env;
  return {
    ref: env.DOM_REF,
    role: env.DOM_ROLE,
    name: env.DOM_NAME,
    selector: env.DOM_SELECTOR,
    action: env.DOM_ACTION ?? 'inspect',
    value: env.DOM_VALUE,
    key: env.DOM_KEY,
    toRef: env.DOM_TO_REF,
    toSelector: env.DOM_TO_SELECTOR,
  };
}

function stepSource(options: DomOptions) {
  if (options.steps) return options.steps;
  if (process.env.DOM_STEPS) return parseEnvSteps(process.env.DOM_STEPS);
  return [envStep()];
}

function normalizeStep(s: RawStep, i: number): DomStep {
  const targeted = Boolean(s.selector || s.ref || s.role || s.name);
  return {
    ref: s.ref || '',
    role: s.role || '',
    name: s.name || '',
    waitTarget: targeted,
    selector: s.selector || (targeted ? '' : DEFAULT_SELECTOR),
    action: s.action ?? 'inspect',
    value: s.value == null ? '' : String(s.value),
    key: s.key || 'Enter',
    toRef: s.toRef || '',
    toSelector: s.toSelector || '',
    index: i + 1,
  };
}

function validateStep(step: DomStep, waitText: string) {
  if (!DOM_ACTIONS.includes(step.action))
    throw new Error(
      `Unsupported action "${step.action}" (step ${step.index}). Use ${DOM_ACTIONS.join(', ')}.`
    );
  if (step.action === 'press') buildKeyPressEvents(step.key); // fail fast on an unknown key
  if (step.action === 'drag' && !step.toRef && !step.toSelector)
    throw new Error(
      'drag needs DOM_TO_REF or DOM_TO_SELECTOR (toRef/toSelector in DOM_STEPS)'
    );
  if (step.action === 'wait' && !step.value && !waitText && !step.waitTarget)
    throw new Error(
      'wait needs DOM_VALUE (or DOM_WAIT_TEXT): text to wait for, "|" = any of'
    );
}

/** The steps to run, normalized; throws on the first invalid one. */
export function readSteps(options: DomOptions, waitText: string) {
  const steps = stepSource(options);
  if (!Array.isArray(steps) || !steps.length)
    throw new Error('steps must be a non-empty array');
  return steps.map((s, i) => {
    const step = normalizeStep(s, i);
    validateStep(step, waitText);
    return step;
  });
}
