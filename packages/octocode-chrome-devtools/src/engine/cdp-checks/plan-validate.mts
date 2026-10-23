// Whole-plan validation for browser-execute: every step is checked before any browser call.
import {
  chromePlanOperations,
  chromeExtractionFields,
} from '../chrome-contract.mjs';
import { DOM_ACTIONS, readDomConfig, readSteps } from './dom-config.mjs';

export const planOperations = chromePlanOperations;

const protocolMember = /^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$/;
const PLAN_FIELDS = [
  'steps',
  'waitMs',
  'settleMs',
  'traceEvents',
  'observe',
  'commandMs',
];
const STEP_FIELDS = [
  'op',
  'url',
  'action',
  'ref',
  'selector',
  'role',
  'name',
  'value',
  'key',
  'toRef',
  'toSelector',
  'after',
  'timeoutMs',
  'frame',
  'session',
  'fields',
  'allowEmpty',
  'method',
  'params',
  'domain',
  'member',
  'id',
  'event',
  'where',
  'listener',
  'count',
  'handle',
  'size',
  'close',
];
const AFTER_OPS = ['goto', 'act', 'wait', 'extract', 'cdp'];
const FRAME_OPS = ['act', 'wait', 'extract', 'cdp', 'listen'];
const SESSION_OPS = [...AFTER_OPS, 'listen', 'readStream'];
const TEXT_FIELDS = [
  'ref',
  'selector',
  'role',
  'name',
  'key',
  'toRef',
  'toSelector',
];
const SCALARS = ['string', 'number', 'boolean'];

const integer = (value, name, min = 0) => {
  if (!Number.isSafeInteger(value) || value < min)
    throw new Error(`Invalid ${name}`);
};
const isRecord = value =>
  Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const object = (value, label) => {
  if (!isRecord(value)) throw new Error(`${label} must be an object`);
};
const fields = (value, allowed, label) => {
  object(value, label);
  for (const key of Object.keys(value))
    if (!allowed.includes(key))
      throw new Error(`Unknown ${label} field: ${key}`);
};
const nonemptyText = value => typeof value === 'string' && Boolean(value);
const allNonemptyText = value =>
  Object.keys(value).length > 0 &&
  Object.values(value).every(v => nonemptyText(v));

const pointerKeys = path =>
  path
    ? path
        .slice(1)
        .split('/')
        .map(k => k.replace(/~1/g, '/').replace(/~0/g, '~'))
    : [];

/** The value at a JSON pointer, or undefined when a key is missing. */
export function jsonPointer(value, path = '') {
  if (
    typeof path !== 'string' ||
    (path && !path.startsWith('/')) ||
    /~(?![01])/u.test(path)
  )
    throw new Error('Invalid result JSON pointer');
  let current = value;
  for (const key of pointerKeys(path))
    current =
      current !== null &&
      typeof current === 'object' &&
      Object.hasOwn(current, key)
        ? current[key]
        : undefined;
  return current;
}

function validateReference(value, { index, steps, listeners }) {
  fields(value, ['$step', '$event', 'pointer'], 'reference');
  jsonPointer({}, value.pointer ?? '');
  if (
    value.$step !== undefined &&
    (!Number.isSafeInteger(value.$step) ||
      value.$step < 1 ||
      value.$step > index ||
      steps[value.$step - 1]?.op !== 'cdp')
  )
    throw new Error('$step must reference an earlier CDP result');
  if (value.$event !== undefined && !listeners.has(value.$event))
    throw new Error('$event must reference an earlier listener');
  if (value.$step !== undefined && value.$event !== undefined)
    throw new Error('Use one reference source');
}

/** Checks every { $step | $event, pointer } reference nested in value. */
function validateReferences(value, scope) {
  if (!value || typeof value !== 'object') return;
  if (Object.hasOwn(value, '$step') || Object.hasOwn(value, '$event'))
    validateReference(value, scope);
  else for (const v of Object.values(value)) validateReferences(v, scope);
}

function validatePlanOptions(plan) {
  if (!isRecord(plan)) throw new Error('Plan must be an object');
  fields(plan, PLAN_FIELDS, 'plan');
  if (!Array.isArray(plan.steps) || !plan.steps.length)
    throw new Error('Plan needs non-empty steps');
  integer(plan.commandMs ?? plan.waitMs ?? 8000, 'commandMs', 1);
  integer(plan.waitMs ?? 8000, 'waitMs', 1);
  integer(plan.settleMs ?? 0, 'settleMs');
  if (plan.traceEvents !== undefined && typeof plan.traceEvents !== 'boolean')
    throw new Error('traceEvents must be boolean');
  if (plan.observe === undefined) return;
  fields(plan.observe, ['network', 'afterMs'], 'observe');
  if (
    plan.observe.network !== undefined &&
    typeof plan.observe.network !== 'boolean'
  )
    throw new Error('Invalid observe.network');
  integer(plan.observe.afterMs ?? 0, 'afterMs');
}

const opFields = op => [
  'op',
  'timeoutMs',
  ...(AFTER_OPS.includes(op) ? ['after'] : []),
  ...(FRAME_OPS.includes(op) ? ['frame'] : []),
  ...(SESSION_OPS.includes(op) ? ['session'] : []),
  ...planOperations[op],
];

function validateStepShape(plan, step, index) {
  if (!isRecord(step)) throw new Error(`Invalid step ${index + 1}`);
  fields(step, STEP_FIELDS, 'step');
  if (!Object.hasOwn(planOperations, step.op))
    throw new Error(`Unsupported op: ${step.op}`);
  fields(step, opFields(step.op), step.op);
  integer(step.timeoutMs ?? plan.waitMs ?? 8000, 'timeoutMs', 1);
}

function validateSession(step, scope) {
  if (step.frame !== undefined) throw new Error('Use one of frame or session');
  const reference =
    isRecord(step.session) &&
    (Object.hasOwn(step.session, '$step') ||
      Object.hasOwn(step.session, '$event'));
  if (typeof step.session !== 'string' && !reference)
    throw new Error('session needs an id or result reference');
  if (step.session === '') throw new Error('session id must be nonempty');
  validateReferences(step.session, scope);
}

function validateListen(step, listeners) {
  if (
    typeof step.id !== 'string' ||
    !/^[a-zA-Z][a-zA-Z0-9-]*$/.test(step.id) ||
    listeners.has(step.id)
  )
    throw new Error('listen needs a unique safe id');
  if (typeof step.event !== 'string' || !protocolMember.test(step.event))
    throw new Error('listen needs Domain.event');
  if (step.where !== undefined) object(step.where, 'where');
  for (const [path, value] of Object.entries(step.where || {})) {
    jsonPointer({}, path);
    if (value !== null && !SCALARS.includes(typeof value))
      throw new Error('where needs scalar values');
  }
  listeners.add(step.id);
}

function validateReadStream(step) {
  const reference =
    isRecord(step.handle) &&
    (step.handle.$step !== undefined || step.handle.$event !== undefined);
  if (typeof step.handle !== 'string' && !reference)
    throw new Error('readStream needs a handle or result reference');
  integer(step.size ?? 65536, 'size', 1);
  if ((step.size ?? 65536) > 1048576)
    throw new Error('Stream size must be <= 1048576');
  if (step.close !== undefined && typeof step.close !== 'boolean')
    throw new Error('close must be boolean');
}

function validateDomTarget(step) {
  if (step.op === 'act' && !DOM_ACTIONS.includes(step.action))
    throw new Error('act needs a supported action');
  const target = step.ref || step.selector || step.role || step.name;
  if (!target && !(step.op === 'wait' && step.value))
    throw new Error('Action needs an explicit target; wait may use text value');
  for (const key of TEXT_FIELDS)
    if (step[key] !== undefined && !nonemptyText(step[key]))
      throw new Error(`${key} needs nonempty text`);
}

function validateDomValue(plan, step) {
  if (
    step.value !== undefined &&
    (!SCALARS.includes(typeof step.value) ||
      (typeof step.value === 'number' && !Number.isFinite(step.value)))
  )
    throw new Error('value needs text, a number or boolean');
  const options = {
    steps: [{ ...step, action: step.op === 'wait' ? 'wait' : step.action }],
    waitMs: step.timeoutMs ?? plan.waitMs ?? 8000,
    waitText: step.after?.text ?? '',
    diff: false,
  };
  readSteps(options, readDomConfig(options).waitText);
}

function validateExtract(step) {
  if (!nonemptyText(step.selector)) throw new Error('extract needs selector');
  if (
    step.fields !== undefined &&
    (!Array.isArray(step.fields) ||
      !step.fields.length ||
      step.fields.some(f => !chromeExtractionFields.includes(f)))
  )
    throw new Error(
      `Invalid extraction fields; supported fields: ${chromeExtractionFields.join(', ')}`
    );
}

function validateStepOps(plan, step, scope) {
  if (step.session !== undefined) validateSession(step, scope);
  if (step.op === 'listen') validateListen(step, scope.listeners);
  if (step.op === 'waitEvent') {
    if (!scope.listeners.has(step.listener))
      throw new Error('waitEvent needs an earlier listener');
    integer(step.count ?? 1, 'count', 1);
  }
  if (step.op === 'readStream') validateReadStream(step);
  validateReferences(step.handle, scope);
  validateReferences(step.params, scope);
  if (step.op === 'goto' && !nonemptyText(step.url))
    throw new Error('goto needs url');
  if (['act', 'wait'].includes(step.op)) {
    validateDomTarget(step);
    validateDomValue(plan, step);
  }
  if (step.op === 'extract') validateExtract(step);
}

function validateCdpFields(step) {
  if (
    step.op === 'cdp' &&
    (typeof step.method !== 'string' || !protocolMember.test(step.method))
  )
    throw new Error('cdp needs Domain.method');
  if (step.params !== undefined && !isRecord(step.params))
    throw new Error('params must be an object');
  if (step.allowEmpty !== undefined && typeof step.allowEmpty !== 'boolean')
    throw new Error('allowEmpty must be boolean');
  if (
    step.op === 'protocol' &&
    (!step.domain || typeof step.domain !== 'string')
  )
    throw new Error('protocol needs domain');
  if (step.member !== undefined && !nonemptyText(step.member))
    throw new Error('member needs nonempty text');
}

function validateFrame(step) {
  fields(step.frame, ['id', 'url', 'selector'], 'frame');
  if (!allNonemptyText(step.frame))
    throw new Error('frame needs nonempty id, URL substring or selector');
  if (step.frame.selector && Object.keys(step.frame).length !== 1)
    throw new Error('frame selector cannot combine with id or url');
  if (step.op === 'listen' && step.frame.selector)
    throw new Error(
      'Selector-frame event listeners are unsupported; use an isolated frame id or URL, or an explicit session with event filters'
    );
  if (step.op === 'goto')
    throw new Error('Use root goto, then scope child-frame steps');
}

/** Validates the whole plan and returns it; throws the first problem found. */
export function validatePlan(plan) {
  validatePlanOptions(plan);
  const listeners = new Set();
  for (const [index, step] of plan.steps.entries()) {
    validateStepShape(plan, step, index);
    validateStepOps(plan, step, { index, steps: plan.steps, listeners });
    validateCdpFields(step);
    if (step.after !== undefined) {
      fields(step.after, ['text', 'selector', 'url'], 'after');
      if (!allNonemptyText(step.after))
        throw new Error('after needs nonempty text, selector or URL');
    }
    if (step.frame !== undefined) validateFrame(step);
  }
  return plan;
}
