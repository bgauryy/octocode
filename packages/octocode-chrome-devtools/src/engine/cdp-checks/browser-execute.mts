import { readFileSync, openSync, writeSync, closeSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createDomOperations } from './dom-operations-check.mjs';
import { run as monitor } from './live-har-monitor.mjs';
import {
  chromePlanOperations,
  chromeExtractionFields,
} from '../chrome-contract.mjs';

const arg = flag => {
  const i = process.argv.indexOf(flag);
  return i < 0 ? null : process.argv[i + 1];
};
export const actions = [
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
const protocolMember = /^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$/;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const integer = (value, name, min = 0) => {
  if (!Number.isSafeInteger(value) || value < min)
    throw new Error(`Invalid ${name}`);
};
const object = (value, label) => {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error(`${label} must be an object`);
};
const fields = (value, allowed, label) => {
  object(value, label);
  for (const key of Object.keys(value))
    if (!allowed.includes(key))
      throw new Error(`Unknown ${label} field: ${key}`);
};

function jsonPointer(value, path = '') {
  if (
    typeof path !== 'string' ||
    (path && !path.startsWith('/')) ||
    /~(?![01])/u.test(path)
  )
    throw new Error('Invalid result JSON pointer');
  for (const key of path
    ? path
        .slice(1)
        .split('/')
        .map(k => k.replace(/~1/g, '/').replace(/~0/g, '~'))
    : [])
    value =
      value !== null && typeof value === 'object' && Object.hasOwn(value, key)
        ? value[key]
        : undefined;
  return value;
}
function validateReferences(value, index, steps, listeners) {
  if (!value || typeof value !== 'object') return;
  if (Object.hasOwn(value, '$step') || Object.hasOwn(value, '$event')) {
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
  } else
    for (const v of Object.values(value))
      validateReferences(v, index, steps, listeners);
}

export const planOperations = chromePlanOperations;

export function validatePlan(plan) {
  if (!plan || typeof plan !== 'object' || Array.isArray(plan))
    throw new Error('Plan must be an object');
  fields(
    plan,
    ['steps', 'waitMs', 'settleMs', 'traceEvents', 'observe', 'commandMs'],
    'plan'
  );
  if (!Array.isArray(plan.steps) || !plan.steps.length)
    throw new Error('Plan needs non-empty steps');
  integer(plan.commandMs ?? plan.waitMs ?? 8000, 'commandMs', 1);
  integer(plan.waitMs ?? 8000, 'waitMs', 1);
  integer(plan.settleMs ?? 0, 'settleMs');
  if (plan.traceEvents !== undefined && typeof plan.traceEvents !== 'boolean')
    throw new Error('traceEvents must be boolean');
  if (plan.observe !== undefined) {
    fields(plan.observe, ['network', 'afterMs'], 'observe');
    if (
      plan.observe.network !== undefined &&
      typeof plan.observe.network !== 'boolean'
    )
      throw new Error('Invalid observe.network');
    integer(plan.observe.afterMs ?? 0, 'afterMs');
  }
  const declaredListeners = new Set();
  for (const [index, step] of plan.steps.entries()) {
    if (!step || typeof step !== 'object' || Array.isArray(step))
      throw new Error(`Invalid step ${index + 1}`);
    fields(
      step,
      [
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
      ],
      'step'
    );
    if (!Object.hasOwn(planOperations, step.op))
      throw new Error(`Unsupported op: ${step.op}`);
    fields(
      step,
      [
        'op',
        'timeoutMs',
        ...(['goto', 'act', 'wait', 'extract', 'cdp'].includes(step.op)
          ? ['after']
          : []),
        ...(['act', 'wait', 'extract', 'cdp', 'listen'].includes(step.op)
          ? ['frame']
          : []),
        ...([
          'goto',
          'act',
          'wait',
          'extract',
          'cdp',
          'listen',
          'readStream',
        ].includes(step.op)
          ? ['session']
          : []),
        ...planOperations[step.op],
      ],
      step.op
    );
    integer(step.timeoutMs ?? plan.waitMs ?? 8000, 'timeoutMs', 1);
    if (step.session !== undefined) {
      if (step.frame !== undefined)
        throw new Error('Use one of frame or session');
      if (
        typeof step.session !== 'string' &&
        (!step.session ||
          typeof step.session !== 'object' ||
          Array.isArray(step.session) ||
          (!Object.hasOwn(step.session, '$step') &&
            !Object.hasOwn(step.session, '$event')))
      )
        throw new Error('session needs an id or result reference');
      if (step.session === '') throw new Error('session id must be nonempty');
      validateReferences(step.session, index, plan.steps, declaredListeners);
    }
    if (step.op === 'listen') {
      if (
        typeof step.id !== 'string' ||
        !/^[a-zA-Z][a-zA-Z0-9-]*$/.test(step.id) ||
        declaredListeners.has(step.id)
      )
        throw new Error('listen needs a unique safe id');
      if (typeof step.event !== 'string' || !protocolMember.test(step.event))
        throw new Error('listen needs Domain.event');
      if (step.where !== undefined) object(step.where, 'where');
      for (const [path, value] of Object.entries(step.where || {})) {
        jsonPointer({}, path);
        if (
          value !== null &&
          !['string', 'number', 'boolean'].includes(typeof value)
        )
          throw new Error('where needs scalar values');
      }
      declaredListeners.add(step.id);
    }
    if (step.op === 'waitEvent') {
      if (!declaredListeners.has(step.listener))
        throw new Error('waitEvent needs an earlier listener');
      integer(step.count ?? 1, 'count', 1);
    }
    if (step.op === 'readStream') {
      if (
        typeof step.handle !== 'string' &&
        (!step.handle ||
          typeof step.handle !== 'object' ||
          Array.isArray(step.handle) ||
          (step.handle.$step === undefined && step.handle.$event === undefined))
      )
        throw new Error('readStream needs a handle or result reference');
      integer(step.size ?? 65536, 'size', 1);
      if ((step.size ?? 65536) > 1048576)
        throw new Error('Stream size must be <= 1048576');
      if (step.close !== undefined && typeof step.close !== 'boolean')
        throw new Error('close must be boolean');
    }
    validateReferences(step.handle, index, plan.steps, declaredListeners);
    validateReferences(step.params, index, plan.steps, declaredListeners);
    if (step.op === 'goto' && (typeof step.url !== 'string' || !step.url))
      throw new Error('goto needs url');
    if (['act', 'wait'].includes(step.op)) {
      if (step.op === 'act' && !actions.includes(step.action))
        throw new Error('act needs a supported action');
      if (
        !step.ref &&
        !step.selector &&
        !step.role &&
        !step.name &&
        !(step.op === 'wait' && step.value)
      )
        throw new Error(
          'Action needs an explicit target; wait may use text value'
        );
      for (const key of [
        'ref',
        'selector',
        'role',
        'name',
        'key',
        'toRef',
        'toSelector',
      ])
        if (
          step[key] !== undefined &&
          (typeof step[key] !== 'string' || !step[key])
        )
          throw new Error(`${key} needs nonempty text`);
      if (
        step.value !== undefined &&
        (!['string', 'number', 'boolean'].includes(typeof step.value) ||
          (typeof step.value === 'number' && !Number.isFinite(step.value)))
      )
        throw new Error('value needs text, a number or boolean');
      createDomOperations({
        steps: [{ ...step, action: step.op === 'wait' ? 'wait' : step.action }],
        waitMs: step.timeoutMs ?? plan.waitMs ?? 8000,
        waitText: step.after?.text ?? '',
        diff: false,
      });
    }
    if (step.op === 'extract') {
      if (typeof step.selector !== 'string' || !step.selector)
        throw new Error('extract needs selector');
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
    if (
      step.op === 'cdp' &&
      (typeof step.method !== 'string' || !protocolMember.test(step.method))
    )
      throw new Error('cdp needs Domain.method');
    if (
      step.params !== undefined &&
      (!step.params ||
        typeof step.params !== 'object' ||
        Array.isArray(step.params))
    )
      throw new Error('params must be an object');
    if (step.allowEmpty !== undefined && typeof step.allowEmpty !== 'boolean')
      throw new Error('allowEmpty must be boolean');
    if (
      step.op === 'protocol' &&
      (!step.domain || typeof step.domain !== 'string')
    )
      throw new Error('protocol needs domain');
    if (
      step.member !== undefined &&
      (typeof step.member !== 'string' || !step.member)
    )
      throw new Error('member needs nonempty text');
    if (step.after !== undefined) {
      fields(step.after, ['text', 'selector', 'url'], 'after');
      if (
        !Object.keys(step.after).length ||
        Object.values(step.after).some(v => typeof v !== 'string' || !v)
      )
        throw new Error('after needs nonempty text, selector or URL');
    }
    if (step.frame !== undefined) {
      fields(step.frame, ['id', 'url', 'selector'], 'frame');
      if (
        !Object.keys(step.frame).length ||
        Object.values(step.frame).some(v => typeof v !== 'string' || !v)
      )
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
  }
  return plan;
}

function scopedSession(cdp, target, sessionId) {
  const listeners = new Map();
  return {
    ...cdp,
    targetInfo: { ...target, id: target.targetId, sessionId },
    async foreground() {
      if (cdp.targetInfo.type === 'browser') {
        const { targetInfo } = await cdp.send(
          'Target.getTargetInfo',
          {},
          sessionId
        );
        const { targetInfos } = await cdp.send('Target.getTargets');
        const byId = new Map(targetInfos.map(info => [info.targetId, info]));
        const seen = new Set();
        let owner = targetInfo;
        while (owner?.type === 'iframe' && !seen.has(owner.targetId)) {
          seen.add(owner.targetId);
          owner = byId.get(owner.parentId);
        }
        if (owner?.type !== 'page')
          throw new Error(
            'Cannot resolve the top-level page owner for this session; select the exact parent page target and scope its child frame'
          );
        return cdp.send('Target.activateTarget', { targetId: owner.targetId });
      }
      // Explicit sessions can belong to OOPIFs; foregrounding is parent-owned
      // unless the scoped target is known to be a top-level page.
      return target.type === 'page'
        ? cdp.send('Page.bringToFront', {}, sessionId)
        : cdp.foreground();
    },
    send(method, params = {}, childSession) {
      return cdp.send(method, params, childSession ?? sessionId);
    },
    on(event, fn) {
      const wrapped = (params, meta: Record<string, any> = {}) => {
        if (meta.sessionId === sessionId) return fn(params, meta);
      };
      listeners.set(fn, { event, wrapped });
      cdp.on(event, wrapped);
    },
    off(event, fn) {
      const entry = listeners.get(fn);
      if (entry) {
        cdp.off(event, entry.wrapped);
        listeners.delete(fn);
      }
    },
    dispose() {
      for (const entry of listeners.values())
        cdp.off(entry.event, entry.wrapped);
      listeners.clear();
    },
  };
}

function inlineScope(cdp, frameTree, executionContextId) {
  return {
    ...cdp,
    targetInfo: {
      ...cdp.targetInfo,
      frameId: frameTree.frame.id,
      url: frameTree.frame.url,
    },
    send(method, params = {}, sessionId) {
      if (sessionId) return cdp.send(method, params, sessionId);
      if (method === 'Runtime.evaluate')
        params = { ...params, contextId: executionContextId };
      if (method === 'Accessibility.getFullAXTree')
        params = { ...params, frameId: frameTree.frame.id };
      if (method === 'Page.getFrameTree') return Promise.resolve({ frameTree });
      return cdp.send(method, params);
    },
    dispose() {},
  };
}

async function frameScope(cdp, frame, timeoutMs, scopes) {
  if (frame.selector) {
    await cdp.send('DOM.enable');
    await cdp.send('Page.enable');
    const { root } = await cdp.send('DOM.getDocument');
    const { nodeIds } = await cdp.send('DOM.querySelectorAll', {
      nodeId: root.nodeId,
      selector: frame.selector,
    });
    if (nodeIds.length !== 1)
      throw new Error(
        `Frame selector needs exactly one match; found ${nodeIds.length}`
      );
    const { node } = await cdp.send('DOM.describeNode', { nodeId: nodeIds[0] });
    if (!node.frameId)
      throw new Error('Frame selector did not match a frame owner');
    const { frameTree } = await cdp.send('Page.getFrameTree');
    const find = tree =>
      tree.frame.id === node.frameId
        ? tree
        : (tree.childFrames || []).map(find).find(Boolean);
    const selected = find(frameTree);
    if (!selected)
      throw new Error(
        'Frame is outside this session; use isolated frame id or URL'
      );
    const { executionContextId } = await cdp.send('Page.createIsolatedWorld', {
      frameId: node.frameId,
      worldName: 'octocode-browser-execute',
    });
    return inlineScope(cdp, selected, executionContextId);
  }
  const started = Date.now();
  let nextProgress = 0;
  for (;;) {
    const { targetInfos } = await cdp.send('Target.getTargets');
    const related = new Set([cdp.targetInfo.id]);
    for (let changed = true; changed;) {
      changed = false;
      for (const target of targetInfos)
        if (related.has(target.parentId) && !related.has(target.targetId)) {
          related.add(target.targetId);
          changed = true;
        }
    }
    const matches = targetInfos.filter(
      t =>
        t.type === 'iframe' &&
        related.has(t.targetId) &&
        (!frame.id || t.targetId === frame.id) &&
        (!frame.url || t.url.includes(frame.url))
    );
    if (matches.length > 1)
      throw new Error(
        `Ambiguous iframe target; select a candidate with frame.id or a unique frame.url: ${JSON.stringify(matches)}`
      );
    if (matches.length === 1) {
      const target = matches[0];
      if (scopes.has(target.targetId)) return scopes.get(target.targetId).scope;
      const { sessionId } = await cdp.send('Target.attachToTarget', {
        targetId: target.targetId,
        flatten: true,
      });
      const scope = scopedSession(cdp, target, sessionId);
      scopes.set(target.targetId, { scope, sessionId });
      return scope;
    }
    if (Date.now() - started >= timeoutMs)
      throw new Error('Iframe target not found within timeout');
    if (Date.now() - started >= nextProgress) {
      console.log(
        `[PROGRESS] waiting iframe elapsedMs=${Date.now() - started}`
      );
      nextProgress += 1000;
    }
    await sleep(100);
  }
}

async function waitCondition(cdp, condition, timeoutMs) {
  const started = Date.now();
  let progress = 0;
  do {
    let response;
    try {
      response = await cdp.send('Runtime.evaluate', {
        returnByValue: true,
        expression: `(() => {
        const c = ${JSON.stringify(condition)};
        const visible = el => { const r = el.getBoundingClientRect(), s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden' && Number(s.opacity || '1') > 0; };
        return { ready: document.readyState !== 'loading', url: location.href,
          matched: (!c.url || location.href === c.url) && (!c.selector || [...document.querySelectorAll(c.selector)].some(visible)) && (!c.text || document.body?.innerText.includes(c.text)) };
      })()`,
      });
    } catch (error) {
      if (
        !/context.*destroyed|Cannot find context|Inspected target navigated/i.test(
          error.message
        )
      )
        throw error;
    }
    if (response?.exceptionDetails)
      throw new Error(
        response.exceptionDetails.exception?.description ||
          response.exceptionDetails.text
      );
    if (response?.result?.value?.ready && response.result.value.matched)
      return { ...response.result.value, elapsedMs: Date.now() - started };
    if (Date.now() - started >= progress) {
      console.log(
        `[PROGRESS] waiting condition=${JSON.stringify(condition)} elapsedMs=${Date.now() - started}`
      );
      progress += 1000;
    }
    await sleep(100);
  } while (Date.now() - started < timeoutMs);
  throw new Error(`Condition timed out: ${JSON.stringify(condition)}`);
}

export async function run(connection, input) {
  const plan = validatePlan(
    input ??
      JSON.parse(
        arg('--plan')
          ? readFileSync(arg('--plan'), 'utf8')
          : process.env.BROWSER_PLAN || 'null'
      )
  );
  const planArtifact = connection.saveArtifact('browser-plan.json', plan).file;
  let stepDeadline = null;
  const cdp = {
    ...connection,
    foreground() {
      return cdp.send('Page.bringToFront');
    },
    send(method, params, sessionId) {
      const remaining =
        stepDeadline === null ? Infinity : stepDeadline - Date.now();
      if (remaining <= 0)
        return Promise.reject(
          new Error('Browser step deadline exceeded before ' + method)
        );
      return connection.send(method, params, sessionId, {
        timeoutMs: Math.min(plan.commandMs ?? plan.waitMs ?? 8000, remaining),
      });
    },
  };
  const results = [],
    scopes = new Map(),
    sessionScopes = new Map(),
    streams = new Map(),
    artifacts = new Map();
  let failure = null,
    protocol;
  const started = Date.now();
  const resolveParams = value => {
    if (!value || typeof value !== 'object') return value;
    if (value.$step !== undefined || value.$event !== undefined) {
      const source =
        value.$step !== undefined
          ? JSON.parse(readFileSync(artifacts.get(value.$step), 'utf8'))
          : streams.get(value.$event)?.last;
      const selected = jsonPointer(source, value.pointer ?? '');
      if (selected === undefined)
        throw new Error('Result reference is unavailable');
      return selected;
    }
    return Array.isArray(value)
      ? value.map(resolveParams)
      : Object.fromEntries(
          Object.entries(value).map(([k, v]) => [k, resolveParams(v)])
        );
  };
  const execute = async () => {
    for (const [index, step] of plan.steps.entries()) {
      const timeout = step.timeoutMs ?? plan.waitMs ?? 8000,
        row: Record<string, any> = {
          index: index + 1,
          op: step.op,
          status: 'running',
          startedAt: new Date().toISOString(),
        };
      results.push(row);
      console.log(
        `[PROGRESS] BROWSER step=${index + 1}/${plan.steps.length} op=${step.op}`
      );
      const stepStarted = Date.now();
      stepDeadline = stepStarted + timeout;
      const progress = setInterval(
        () =>
          console.log(
            `[PROGRESS] BROWSER step=${index + 1} op=${step.op} elapsedMs=${Date.now() - stepStarted}`
          ),
        1000
      );
      try {
        let target = step.frame
          ? await frameScope(cdp, step.frame, timeout, scopes)
          : cdp;
        if (step.session !== undefined) {
          const sessionId = resolveParams(step.session);
          if (typeof sessionId !== 'string' || !sessionId)
            throw new Error('Session reference is not an id');
          if (!sessionScopes.has(sessionId))
            sessionScopes.set(
              sessionId,
              scopedSession(cdp, { targetId: null, type: 'session' }, sessionId)
            );
          target = sessionScopes.get(sessionId);
        }
        row.targetId = target.targetInfo.id;
        row.source = {
          selectedUrl: target.targetInfo.url ?? null,
          frameId: target.targetInfo.frameId ?? null,
        };
        if (target.targetInfo.sessionId)
          row.sessionId = target.targetInfo.sessionId;
        if (step.op === 'readStream') {
          const handle = resolveParams(step.handle);
          if (typeof handle !== 'string')
            throw new Error('Stream reference is not a handle');
          const file = join(cdp.outputDir, `stream-${index + 1}.jsonl`),
            fd = openSync(file, 'wx', 0o600);
          row.artifact = file;
          row.chunks = 0;
          row.complete = false;
          let readError, cleanupError;
          try {
            for (;;) {
              const chunk = await target.send('IO.read', {
                handle,
                size: step.size ?? 65536,
              });
              const bytes = Buffer.from(
                JSON.stringify({ index: row.chunks++, ...chunk }) + '\n'
              );
              for (let at = 0; at < bytes.length;)
                at += writeSync(fd, bytes, at, bytes.length - at);
              if (chunk.eof) {
                row.complete = true;
                break;
              }
            }
          } catch (error) {
            readError = error;
            throw error;
          } finally {
            // Cleanup has its own bounded deadline, even if the step timed out.
            if (step.close !== false)
              try {
                await connection.send(
                  'IO.close',
                  { handle },
                  target.targetInfo.sessionId,
                  { timeoutMs: 1000 }
                );
              } catch (error) {
                cleanupError = error;
                row.cleanupError = error.message;
              }
            closeSync(fd);
            console.log(`[ARTIFACT] STREAM ${file}`);
            console.log(
              '[NEXT] ' +
                JSON.stringify({
                  continue: {
                    command: process.execPath,
                    args: [
                      fileURLToPath(
                        new URL('../artifact-query.mjs', import.meta.url)
                      ),
                      '--file',
                      file,
                      '--format',
                      'text',
                    ],
                  },
                })
            );
            if (cleanupError && !readError) throw cleanupError;
          }
        } else if (step.op === 'listen') {
          const file = join(cdp.outputDir, `events-${step.id}.jsonl`),
            fd = openSync(file, 'wx', 0o600);
          const stream: Record<string, any> = {
            target,
            file,
            fd,
            observed: 0,
            matched: 0,
          };
          stream.handler = (params, meta: Record<string, any> = {}) => {
            const bytes = Buffer.from(
              JSON.stringify({
                at: new Date().toISOString(),
                params,
                sessionId: meta.sessionId ?? null,
              }) + '\n'
            );
            for (let at = 0; at < bytes.length;)
              at += writeSync(fd, bytes, at, bytes.length - at);
            stream.observed++;
            if (
              Object.entries(step.where || {}).every(
                ([path, expected]) => jsonPointer(params, path) === expected
              )
            ) {
              stream.matched++;
              stream.last = params;
            }
          };
          target.on(step.event, stream.handler);
          stream.event = step.event;
          streams.set(step.id, stream);
          row.artifact = file;
        } else if (step.op === 'waitEvent') {
          const stream = streams.get(step.listener),
            begin = Date.now();
          while (stream.matched < (step.count ?? 1)) {
            if (Date.now() - begin >= timeout)
              throw new Error('Event condition timed out');
            await sleep(50);
          }
          row.observed = stream.observed;
          row.matched = stream.matched;
        } else if (step.op === 'goto') {
          await target.send('Page.enable');
          let handler, timer;
          const committed = new Promise(resolve => {
            handler = ({ frame }) => {
              if (!frame.parentId) resolve(true);
            };
            target.on('Page.frameNavigated', handler);
            timer = setTimeout(() => resolve(false), timeout);
          });
          try {
            const nav = await target.send('Page.navigate', { url: step.url });
            if (nav.errorText) throw new Error(nav.errorText);
            if (nav.loaderId && !(await committed))
              throw new Error('Navigation did not commit within timeout');
            row.condition = await waitCondition(
              target,
              step.after || {},
              timeout
            );
          } finally {
            clearTimeout(timer);
            target.off('Page.frameNavigated', handler);
          }
          target.targetInfo = { ...target.targetInfo, url: row.condition.url };
        } else if (step.op === 'act' || step.op === 'wait') {
          const result = await createDomOperations({
            steps: [
              { ...step, action: step.op === 'wait' ? 'wait' : step.action },
            ],
            waitMs: timeout,
            settleMs: plan.settleMs ?? 0,
            waitText: step.op === 'act' ? (step.after?.text ?? '') : '',
            traceEvents: plan.traceEvents ?? false,
            literalWaitText: true,
            diff: false,
            artifactName: `dom-step-${index + 1}.json`,
          }).run(target);
          row.artifact = result.artifactPath;
          if (!result.ok)
            throw new Error(
              'DOM action or wait failed; action will not be repeated'
            );
          if (step.after) {
            const condition =
              step.op === 'act'
                ? Object.fromEntries(
                    Object.entries(step.after).filter(([key]) => key !== 'text')
                  )
                : step.after;
            row.condition = {
              ...(step.after.text
                ? { text: step.after.text, verified: true }
                : {}),
              ...(Object.keys(condition).length
                ? await waitCondition(target, condition, timeout)
                : {}),
            };
          }
        } else if (step.op === 'extract') {
          if (step.after)
            row.condition = await waitCondition(target, step.after, timeout);
          const evaluated = await target.send('Runtime.evaluate', {
            returnByValue: true,
            expression: `(() => {
            const fields = ${JSON.stringify(step.fields || ['text', 'href'])};
            const rows = [...document.querySelectorAll(${JSON.stringify(step.selector)})].map(el => Object.fromEntries(fields.map(field => [field,
              field === 'text' ? el.innerText || el.textContent || '' : field === 'href' ? el.href || null : field === 'value' ? el.value ?? null : field === 'role' ? el.getAttribute('role') : el.getAttribute('aria-label') || el.getAttribute('name') || null])));
            return {rows, url: location.href, title: document.title};
          })()`,
          });
          if (evaluated.exceptionDetails) {
            row.artifact = cdp.saveArtifact(
              `extract-error-${index + 1}.json`,
              evaluated
            ).file;
            throw new Error(
              evaluated.exceptionDetails.exception?.description ||
                evaluated.exceptionDetails.text
            );
          }
          const value = evaluated.result?.value;
          const rows = Array.isArray(value) ? value : value?.rows;
          if (typeof value?.url === 'string') {
            row.source = { ...row.source, url: value.url, title: value.title };
            target.targetInfo = { ...target.targetInfo, url: value.url };
          }
          if (!Array.isArray(rows))
            throw new Error('Extraction returned no rows');
          if (!rows.length && !step.allowEmpty)
            throw new Error(
              'Extraction matched zero elements; use allowEmpty for an expected empty result'
            );
          row.count = rows.length;
          row.artifact = cdp.saveArtifact(
            `extract-${index + 1}.json`,
            rows
          ).file;
        } else if (step.op === 'cdp') {
          const result = await target.send(
            step.method,
            resolveParams(step.params || {})
          );
          row.artifact = cdp.saveArtifact(`cdp-${index + 1}.json`, result).file;
          artifacts.set(index + 1, row.artifact);
          if (result.exceptionDetails)
            throw new Error(
              result.exceptionDetails.exception?.description ||
                result.exceptionDetails.text
            );
          if (step.after)
            row.condition = await waitCondition(target, step.after, timeout);
        } else {
          if (!protocol) {
            protocol = await cdp.protocol();
            cdp.saveArtifact('installed-protocol.json', protocol);
          }
          const domain = protocol.domains.find(d => d.domain === step.domain);
          if (!domain) throw new Error('Protocol domain not found');
          const selected = step.member
            ? {
                domain: domain.domain,
                commands:
                  domain.commands?.filter(c => c.name === step.member) || [],
                events:
                  domain.events?.filter(e => e.name === step.member) || [],
                types: domain.types || [],
              }
            : domain;
          if (
            step.member &&
            !selected.commands.length &&
            !selected.events.length
          )
            throw new Error('Protocol member not found');
          row.artifact = cdp.saveArtifact(
            `protocol-${index + 1}.json`,
            selected
          ).file;
        }
        row.status = 'complete';
      } catch (error) {
        if (error.protocolError)
          row.artifact = cdp.saveArtifact(
            `protocol-error-${index + 1}.json`,
            error.protocolError
          ).file;
        row.status = 'failed';
        row.error = error.message;
        failure = {
          step: index + 1,
          error: error.message,
          ...(/CDP timeout|step deadline/.test(error.message)
            ? {
                outcomeUncertain: ['act', 'cdp', 'goto', 'readStream'].includes(
                  step.op
                ),
                instruction:
                  'Inspect state before any retry; an acknowledged outcome is unavailable.',
              }
            : {}),
        };
        break;
      } finally {
        stepDeadline = null;
        clearInterval(progress);
        row.finishedAt = new Date().toISOString();
        row.source = {
          ...row.source,
          ...(row.condition?.url ? { url: row.condition.url } : {}),
          capturedAt: row.finishedAt,
        };
      }
    }
  };
  try {
    if (plan.observe?.network)
      await monitor(cdp, {
        onReady: execute,
        monitorMs: plan.observe.afterMs ?? 0,
        url: '',
      });
    else await execute();
  } catch (error) {
    failure ??= { error: error.message };
  } finally {
    for (const stream of streams.values()) {
      stream.target.off(stream.event, stream.handler);
      closeSync(stream.fd);
      console.log(`[ARTIFACT] EVENTS ${stream.file}`);
      console.log(
        '[NEXT] ' +
          JSON.stringify({
            continue: {
              command: process.execPath,
              args: [
                fileURLToPath(
                  new URL('../artifact-query.mjs', import.meta.url)
                ),
                '--file',
                stream.file,
                '--format',
                'text',
              ],
            },
          })
      );
    }
    for (const scope of sessionScopes.values()) scope.dispose();
    for (const { scope, sessionId } of scopes.values()) {
      scope.dispose();
      if (sessionId)
        await cdp
          .send('Target.detachFromTarget', { sessionId })
          .catch(() => {});
    }
    const result = {
      ok: !failure,
      failure,
      planArtifact,
      elapsedMs: Date.now() - started,
      requestedSteps: plan.steps.length,
      completedSteps: results.filter(r => r.status === 'complete').length,
      steps: results,
      coverage: {
        executedSteps: results.length,
        unvisitedSteps: plan.steps.length - results.length,
        boundary:
          'selected targets and frames during this plan; no site-wide coverage',
      },
      eventCoverage: [...streams.entries()].map(([id, stream]) => ({
        id,
        event: stream.event,
        observed: stream.observed,
        matched: stream.matched,
        artifact: stream.file,
        boundary: 'listener registration to flow completion',
      })),
    };
    cdp.saveArtifact('browser-result.json', result);
    if (failure) {
      process.exitCode = 1;
      console.log(`[FINDING] BROWSER_STOPPED ${JSON.stringify(failure)}`);
    } else
      console.log(
        `[METRIC] BROWSER completed=${result.completedSteps}/${result.requestedSteps} elapsedMs=${result.elapsedMs}`
      );
  }
  return { ok: !failure, results };
}
