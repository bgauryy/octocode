#!/usr/bin/env node
// Checks a coordination plan's dependency graph before dispatch (circular waits, missing producers, writer
// conflicts, tasks that can never become ready), lists the ready tasks, follows a changed input to the evidence it
// makes stale, and validates a resume checkpoint. Built-ins only.
import { readFileSync } from 'node:fs';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const USAGE = `Usage: node dep-graph.mjs --plan <plan.json> [--changed <task-or-artifact>]...
       node dep-graph.mjs --validate-checkpoint <checkpoint.json> [--changed <task-or-artifact>]...

plan.json, one entry per task from its task contract (unknown fields are kept but ignored):
  {"available": ["<artifact>"],             artifacts that exist without a producer (user input, existing files)
   "tasks": [{"id": "<task>",                unique
              "owner": "<worker>",           who executes it; tasks with one owner are serialized by that owner
              "objective": "...", "acceptance": "...",   text, not checked
              "inputs": ["<artifact>"],      artifacts the task needs before it can start
              "output": "<artifact>" | ["<artifact>"],   what it produces; name each version separately
              "scope": ["<path or resource>"],          what it may change; a glob counts from its fixed prefix
              "status": "pending"}]}         pending | running | reported | accepted | failed | blocked | cancelled
  reported: the worker says done, not yet verified; accepted: its acceptance check passed on current inputs;
  blocked: waiting on authority or input outside the plan. Only an accepted producer makes its output available.

--plan prints {"ok","counts","changed","cycles","missingProducers","writerConflicts","ready","waiting","neverReady","stale"}:
  cycles            [[task ids]]: tasks that wait on each other through inputs nobody can supply yet
  missingProducers  [{"task","input"}]: an input no task outputs and "available" does not list
  writerConflicts   [{"tasks":[a,b],"boundary","kind"}]: kind "output" (two producers of one artifact) or "scope"
                    (overlapping scope); reported when neither task depends on the other, their owners differ and
                    they are not both accepted (cancelled tasks never conflict)
  ready             pending tasks whose inputs are all available: dispatchable once their conflicts are resolved
  waiting           [{"id","on":[inputs],"blocked"}]: pending or blocked tasks that can still become ready
  neverReady        [{"id","cause","via"}]: cause cycle, missing-producer or cancelled-producer; via is the chain
  stale             with --changed: [{"id","status","via"}] downstream tasks whose evidence or running work used the
                    changed input (status running, reported, accepted or failed); tasks not listed keep their evidence.
                    Their outputs count as unavailable for ready and waiting.
--validate-checkpoint reads a checkpoint written before handoff or compaction:
  {"goal": "...", "acceptance": ["..."], "plan": {plan.json shape}, "next": ["next safe action"],
   "handles": [{"task","ref","kind"}],     a running or reported task needs one, to reconcile it
   "evidence": [{"task","ref","status"}],  status current | stale; an accepted task needs current evidence
   "unresolvedEffects": [{"task","operation","check"}],   writes whose outcome is unknown: neither that task nor
                                           anything downstream of it may be accepted
   "constraints": [], "approvals": [], "files": []}       optional lists of text
  and prints {"ok","errors":[{"field","message"}],"plan":{--plan output}}.
Reads the file only; changes nothing.
Exit 0: no cycle, missing producer, writer conflict or never-ready task (and, for a checkpoint, no error).
1: at least one of those. 2: invalid input (unreadable JSON, a plan field of the wrong type, a duplicate or unknown
task id, an unknown status or --changed name): {"ok":false,"error":{"code","message"}}.`;

const STATUSES = new Set(['pending', 'running', 'reported', 'accepted', 'failed', 'blocked', 'cancelled']);
const HOLDS_EVIDENCE = new Set(['running', 'reported', 'accepted', 'failed']);

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const strings = (value, where) => {
  if (value === undefined) return [];
  const list = typeof value === 'string' ? [value] : value;
  if (!Array.isArray(list) || !list.every(item => typeof item === 'string' && item.length)) throw new InputError('invalid-plan', `${where}: expected a string or an array of non-empty strings`);
  return list;
};

/** Validated plan: tasks by id (inputs, outputs, scope, status, owner) and the initially available artifacts. */
export function loadPlan(plan, where = 'plan') {
  if (!plan || typeof plan !== 'object' || !Array.isArray(plan.tasks)) throw new InputError('invalid-plan', `${where}: expected {"tasks": [...]}`);
  const tasks = new Map();
  plan.tasks.forEach((task, index) => {
    const at = `${where}.tasks[${index}]`;
    if (!task || typeof task !== 'object' || typeof task.id !== 'string' || !task.id) throw new InputError('invalid-plan', `${at}: a task needs a non-empty string id`);
    if (tasks.has(task.id)) throw new InputError('duplicate-task', `${at}: duplicate task id ${task.id}`);
    const status = task.status ?? 'pending';
    if (!STATUSES.has(status)) throw new InputError('unknown-status', `${at}: status ${JSON.stringify(status)} is not one of ${[...STATUSES].join(', ')}`);
    if (task.owner !== undefined && typeof task.owner !== 'string') throw new InputError('invalid-plan', `${at}.owner: expected a string`);
    tasks.set(task.id, { id: task.id, owner: task.owner ?? null, status, inputs: strings(task.inputs, `${at}.inputs`), outputs: strings(task.output, `${at}.output`), scope: strings(task.scope, `${at}.scope`) });
  });
  return { tasks, available: new Set(strings(plan.available, `${where}.available`)) };
}

/** Strongly connected components with more than one task, or a task that waits on itself (Tarjan). */
function cyclesOf(nodes, edges) {
  let index = 0;
  const order = new Map();
  const low = new Map();
  const stack = [];
  const onStack = new Set();
  const cycles = [];
  const visit = node => {
    order.set(node, index);
    low.set(node, index++);
    stack.push(node);
    onStack.add(node);
    for (const next of edges.get(node) ?? []) {
      if (!order.has(next)) { visit(next); low.set(node, Math.min(low.get(node), low.get(next))); } else if (onStack.has(next)) low.set(node, Math.min(low.get(node), order.get(next)));
    }
    if (low.get(node) !== order.get(node)) return;
    const component = [];
    let member;
    do { member = stack.pop(); onStack.delete(member); component.push(member); } while (member !== node);
    if (component.length > 1 || (edges.get(node) ?? new Set()).has(node)) cycles.push(component.toSorted());
  };
  for (const node of nodes) if (!order.has(node)) visit(node);
  return cycles.toSorted((a, b) => a[0].localeCompare(b[0]));
}

const prefixOf = entry => {
  const glob = entry.search(/[*?[{]/);
  return (glob === -1 ? entry : entry.slice(0, glob)).replace(/\/+$/, '');
};
const overlap = (a, b) => {
  const [x, y] = [prefixOf(a), prefixOf(b)];
  if (!x || !y) return x || y || '*';
  if (x === y) return x;
  if (y.startsWith(`${x}/`)) return y;
  if (x.startsWith(`${y}/`)) return x;
  return null;
};

export function analyzePlan({ tasks, available }, changed = []) {
  const producers = new Map(); // artifact → task ids
  for (const task of tasks.values()) for (const output of task.outputs) producers.set(output, [...(producers.get(output) ?? []), task.id]);
  for (const name of changed) if (!tasks.has(name) && !producers.has(name) && !available.has(name) && ![...tasks.values()].some(task => task.inputs.includes(name))) throw new InputError('unknown-changed', `--changed ${name}: not a task or artifact in the plan`);

  // Dependencies: consumer → producers of each input, whatever their status.
  const dependsOn = new Map([...tasks.keys()].map(id => [id, new Set()]));
  for (const task of tasks.values()) for (const input of task.inputs) for (const producer of producers.get(input) ?? []) dependsOn.get(task.id).add(producer);
  const consumersOf = new Map([...tasks.keys()].map(id => [id, new Set()]));
  for (const [consumer, set] of dependsOn) for (const producer of set) consumersOf.get(producer).add(consumer);

  // Stale: everything downstream of a changed task or artifact.
  const stale = new Map(); // id → via
  const queue = [];
  for (const name of changed) {
    if (tasks.has(name)) for (const consumer of consumersOf.get(name)) queue.push([consumer, [name]]);
    for (const task of tasks.values()) if (task.inputs.includes(name)) queue.push([task.id, [name]]);
  }
  const reached = new Set();
  while (queue.length) {
    const [id, via] = queue.shift();
    if (reached.has(id)) continue;
    reached.add(id);
    if (HOLDS_EVIDENCE.has(tasks.get(id).status)) stale.set(id, [...via, id]);
    for (const consumer of consumersOf.get(id)) queue.push([consumer, [...via, id]]);
  }
  const accepted = id => tasks.get(id).status === 'accepted' && !stale.has(id);
  const availableNow = input => available.has(input) || (producers.get(input) ?? []).some(accepted);

  // Circular waits: a task waits on the producers of every input it cannot use yet.
  const waitsOn = new Map();
  for (const task of tasks.values()) {
    if (accepted(task.id) || task.status === 'cancelled') continue;
    waitsOn.set(task.id, new Set(task.inputs.filter(input => !availableNow(input)).flatMap(input => producers.get(input) ?? [])));
  }
  const cycles = cyclesOf([...waitsOn.keys()], waitsOn);
  const inCycle = new Set(cycles.flat());

  const missingProducers = [];
  for (const task of tasks.values()) for (const input of task.inputs) if (!available.has(input) && !producers.has(input)) missingProducers.push({ task: task.id, input });

  // Never ready: some input can never become available. The recursion follows wait edges only, and every wait
  // cycle is already in inCycle, so it terminates.
  const verdict = new Map(); // id → null (can finish) | { cause, via }
  const blocker = id => {
    if (verdict.has(id)) return verdict.get(id);
    const task = tasks.get(id);
    let result = null;
    if (accepted(id)) result = null;
    else if (task.status === 'cancelled') result = { cause: 'cancelled-producer', via: [id] };
    else if (inCycle.has(id)) result = { cause: 'cycle', via: [id] };
    else {
      for (const input of task.inputs) {
        if (availableNow(input)) continue;
        const sources = producers.get(input) ?? [];
        if (!sources.length) { result = { cause: 'missing-producer', via: [input] }; break; }
        const blocked = sources.map(blocker);
        if (blocked.every(Boolean)) { result = { cause: blocked[0].cause, via: [input, ...blocked[0].via] }; break; }
      }
    }
    verdict.set(id, result);
    return result;
  };
  const neverReady = [];
  const ready = [];
  const waiting = [];
  for (const task of tasks.values()) {
    if (task.status !== 'pending' && task.status !== 'blocked') continue;
    const never = blocker(task.id);
    if (never) { neverReady.push({ id: task.id, ...never }); continue; }
    const on = task.inputs.filter(input => !availableNow(input));
    if (task.status === 'pending' && !on.length) ready.push(task.id);
    else waiting.push({ id: task.id, on, blocked: task.status === 'blocked' });
  }

  // Writers: two producers of one artifact, or overlapping scope, with no dependency ordering them.
  const ancestors = new Map();
  const ancestorsOf = id => {
    if (ancestors.has(id)) return ancestors.get(id);
    const found = new Set();
    ancestors.set(id, found);
    for (const producer of dependsOn.get(id)) { found.add(producer); for (const above of ancestorsOf(producer)) found.add(above); }
    return found;
  };
  const writerConflicts = [];
  const list = [...tasks.values()];
  for (let i = 0; i < list.length; i++) {
    for (let j = i + 1; j < list.length; j++) {
      const [a, b] = [list[i], list[j]];
      if (a.status === 'cancelled' || b.status === 'cancelled' || (a.status === 'accepted' && b.status === 'accepted')) continue;
      if ((a.owner && a.owner === b.owner) || ancestorsOf(a.id).has(b.id) || ancestorsOf(b.id).has(a.id)) continue;
      for (const output of a.outputs.filter(item => b.outputs.includes(item))) writerConflicts.push({ tasks: [a.id, b.id], boundary: output, kind: 'output' });
      const shared = new Set();
      for (const x of a.scope) for (const y of b.scope) { const boundary = overlap(x, y); if (boundary) shared.add(boundary); }
      for (const boundary of shared) writerConflicts.push({ tasks: [a.id, b.id], boundary, kind: 'scope' });
    }
  }

  const staleRows = [...stale].map(([id, via]) => ({ id, status: tasks.get(id).status, via }));
  const result = { cycles, missingProducers, writerConflicts, ready, waiting, neverReady, stale: staleRows };
  const counts = Object.fromEntries(Object.entries(result).map(([key, value]) => [key, value.length]));
  const defects = cycles.length + missingProducers.length + writerConflicts.length + neverReady.length;
  return { ok: defects === 0, counts, changed, ...result };
}

const textList = (value, field, errors, { required = false } = {}) => {
  if (value === undefined) { if (required) errors.push({ field, message: 'required: a non-empty array of strings' }); return; }
  if (!Array.isArray(value) || !value.every(item => typeof item === 'string' && item.trim())) errors.push({ field, message: 'expected an array of non-empty strings' });
  else if (required && !value.length) errors.push({ field, message: 'required: a non-empty array of strings' });
};

const records = (value, field, errors, fields) => {
  if (value === undefined) return [];
  if (!Array.isArray(value)) { errors.push({ field, message: 'expected an array' }); return []; }
  return value.filter((item, index) => {
    const missing = fields.filter(name => typeof item?.[name] !== 'string' || !item[name]);
    if (missing.length) errors.push({ field: `${field}[${index}]`, message: `needs string ${missing.join(', ')}` });
    return !missing.length;
  });
};

/** Checkpoint shape and consistency errors, plus the plan analysis. */
export function validateCheckpoint(checkpoint, changed = []) {
  if (!checkpoint || typeof checkpoint !== 'object' || Array.isArray(checkpoint)) throw new InputError('invalid-checkpoint', 'checkpoint: expected a JSON object');
  const errors = [];
  if (typeof checkpoint.goal !== 'string' || !checkpoint.goal.trim()) errors.push({ field: 'goal', message: 'required: the user outcome in one string' });
  textList(checkpoint.acceptance, 'acceptance', errors, { required: true });
  textList(checkpoint.next, 'next', errors, { required: true });
  for (const field of ['constraints', 'approvals', 'files']) textList(checkpoint[field], field, errors);
  let plan = null;
  let analysis = null;
  try {
    plan = loadPlan(checkpoint.plan, 'plan');
    analysis = analyzePlan(plan, changed);
  } catch (error) {
    if (!(error instanceof InputError) || error.code === 'unknown-changed') throw error;
    errors.push({ field: 'plan', message: error.message });
  }
  const handles = records(checkpoint.handles, 'handles', errors, ['task', 'ref']);
  const evidence = records(checkpoint.evidence, 'evidence', errors, ['task', 'ref', 'status']);
  const effects = records(checkpoint.unresolvedEffects, 'unresolvedEffects', errors, ['task', 'operation']);
  if (plan) {
    const known = (list, field) => list.forEach((item, index) => { if (!plan.tasks.has(item.task)) errors.push({ field: `${field}[${index}].task`, message: `${item.task} is not a task in the plan` }); });
    known(handles, 'handles');
    known(evidence, 'evidence');
    known(effects, 'unresolvedEffects');
    evidence.forEach((item, index) => { if (item.status !== 'current' && item.status !== 'stale') errors.push({ field: `evidence[${index}].status`, message: 'expected current or stale' }); });
    for (const task of plan.tasks.values()) {
      if ((task.status === 'running' || task.status === 'reported') && !handles.some(item => item.task === task.id)) errors.push({ field: 'handles', message: `${task.id} is ${task.status} but has no handle to reconcile it` });
      if (task.status === 'accepted' && !evidence.some(item => item.task === task.id && item.status === 'current')) errors.push({ field: 'evidence', message: `${task.id} is accepted but has no current evidence` });
    }
    const effectTasks = effects.map(item => item.task).filter(id => plan.tasks.has(id));
    if (effectTasks.length) {
      const downstream = analyzePlan(plan, [...new Set(effectTasks)]).stale.map(row => row.id);
      for (const id of new Set([...effectTasks, ...downstream])) {
        if (plan.tasks.get(id).status === 'accepted') errors.push({ field: 'unresolvedEffects', message: `${id} is accepted while an effect it depends on is unknown; keep it unverified` });
      }
    }
  }
  return { ok: errors.length === 0 && (analysis?.ok ?? false), errors, plan: analysis };
}

function readJson(path) {
  let text;
  try { text = readFileSync(path, 'utf8'); } catch (error) { throw new InputError('unreadable', `${path}: ${error.code ?? errorMessage(error)}`); }
  try { return JSON.parse(text); } catch (error) { throw new InputError('invalid-json', `${path}: ${errorMessage(error)}`); }
}

function parseArgs(argv) {
  const options = { plan: null, checkpoint: null, changed: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { console.log(USAGE); process.exit(0); }
    const value = argv[++i];
    if (value === undefined || value.startsWith('--')) fail('invalid-args', `${arg} needs a value; see --help`, 2);
    if (arg === '--plan') options.plan = value;
    else if (arg === '--validate-checkpoint') options.checkpoint = value;
    else if (arg === '--changed') options.changed.push(value);
    else fail('invalid-args', `unknown argument ${arg}; see --help`, 2);
  }
  if (Boolean(options.plan) === Boolean(options.checkpoint)) fail('invalid-args', 'give exactly one of --plan or --validate-checkpoint; see --help', 2);
  return options;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  let result;
  try {
    result = options.plan
      ? analyzePlan(loadPlan(readJson(options.plan)), options.changed)
      : validateCheckpoint(readJson(options.checkpoint), options.changed);
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('analysis-failed', errorMessage(error), 2);
  }
  console.log(JSON.stringify(result, null, 2));
  process.exit(result.ok ? 0 : 1);
}

if (isMainModule(import.meta.url)) main();
