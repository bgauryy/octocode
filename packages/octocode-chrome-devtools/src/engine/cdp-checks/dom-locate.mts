// Finding a step's element (snapshot ref, role + name, or CSS selector) and checking it in the page.
import { readTargetSnapshot } from './snapshot-resource.mjs';
import { collectRefs } from '../ax-snapshot.mjs';
import { coreBodyJs } from './dom-page-scripts.mjs';
import { setTimeout as sleep } from 'node:timers/promises';
import type { Payload, Session } from '../cdp-connection.mjs';
import type {
  DomConfig,
  DomContext,
  DomStep,
  LocatedStep,
  SnapshotRef,
  StepDetails,
} from './dom-types.mjs';

/** A missed lookup, reported as the step's result. */
type Miss = { miss: LocatedStep };

// Refs come from the latest page-snapshot.json (or refs added by an earlier [NEW] diff).
export function resolveRefEntry(cdp: Session, ref: string): SnapshotRef {
  const snap = readTargetSnapshot(cdp);
  if (!snap)
    throw new Error(
      'No page-snapshot resource found for this target — run scripts/cdp-checks/page-snapshot.mjs on the same --port first.'
    );
  if (!snap.data.targetId || snap.data.targetId !== cdp.targetInfo.id)
    throw new Error(
      'Snapshot refs belong to another target or lack target identity; snapshot this target first'
    );
  const entry = snap.data.refs?.[ref];
  if (!entry)
    throw new Error(
      `Ref ${ref} not found in ${snap.path}. Available: ${Object.keys(
        snap.data.refs ?? {}
      )
        .slice(0, 40)
        .join(', ')}`
    );
  return entry;
}

const valueOf = (res: Payload | null): StepDetails =>
  res?.result?.value ?? {
    found: false,
    error:
      res?.exceptionDetails?.exception?.description?.split('\n')[0] ??
      'No value returned',
  };

const checkArgs = (step: DomStep, cfg: DomConfig) =>
  [step.action, step.value, cfg.stabilityMs, cfg.mode, step.key]
    .map(v => JSON.stringify(v))
    .join(', ');

async function evaluateCheck(
  cdp: Session,
  label: string,
  expression: string
): Promise<LocatedStep> {
  const details = valueOf(
    await cdp.send('Runtime.evaluate', {
      awaitPromise: true,
      returnByValue: true,
      expression,
    })
  );
  const handle = details.found
    ? await cdp
        .send('Runtime.evaluate', { expression: 'globalThis.__octoTarget' })
        .catch(() => null)
    : null;
  return { label, details, objectId: handle?.result?.objectId ?? null };
}

function locateBySelector(cdp: Session, step: DomStep, cfg: DomConfig) {
  return evaluateCheck(
    cdp,
    `selector:${step.selector}`,
    `(async () => {
      ${coreBodyJs(cfg.traceEvents)}
      const matches = [...document.querySelectorAll(${JSON.stringify(step.selector)})];
      if (matches.length > 1 && ${JSON.stringify(!['inspect', 'wait'].includes(step.action))}) return { found: false, error: 'Ambiguous selector: ' + matches.length + ' matches', ambiguous: true };
      const element = ${JSON.stringify(step.action === 'wait')} ? matches.find(el => isVisible(el) && (!${JSON.stringify(step.value)} || (el.innerText || '').includes(${JSON.stringify(step.value)}))) || matches[0] : matches[0];
      if (!element) return { selector: ${JSON.stringify(step.selector)}, found: false };
      const details = await checkElement(element, ${checkArgs(step, cfg)});
      details.selector = ${JSON.stringify(step.selector)};
      return details;
    })()`
  );
}

/** The unique snapshot-free entry for role + name, or the not-found result. */
async function entryByRoleName(
  cdp: Session,
  step: DomStep,
  label: string
): Promise<Miss | { miss?: undefined; entry: SnapshotRef }> {
  const candidates = (await collectRefs(cdp, {})).useful.filter(
    n =>
      (!step.role || n.role === step.role) &&
      (!step.name || n.fullName === step.name)
  );
  if (candidates.length !== 1)
    return {
      miss: {
        label,
        details: {
          found: false,
          ambiguous: candidates.length > 1,
          error: `${candidates.length} elements match role/name`,
          candidates: candidates.map(n => ({
            role: n.role,
            name: n.fullName,
            backendDOMNodeId: n.backendDOMNodeId,
          })),
        },
        objectId: null,
      },
    };
  return { entry: { ...candidates[0], name: candidates[0].fullName } };
}

/** The entry's live node, or null when it is gone or detached (logged as a stale ref). */
async function liveObject(
  cdp: Session,
  step: DomStep,
  entry: SnapshotRef,
  log: DomContext['log']
): Promise<{ objectId: string } | null> {
  const recovering = `recovering by role=${JSON.stringify(entry.role)} name=${JSON.stringify(entry.name)}`;
  let object: { objectId?: string } | null = null;
  try {
    ({ object } = await cdp.send('DOM.resolveNode', {
      backendNodeId: entry.backendDOMNodeId,
    }));
  } catch {
    // Client-rendered pages replace nodes after the snapshot; recover by role+name.
    log(`[FINDING] STALE_SNAPSHOT_REF ${step.ref}; ${recovering}`);
  }
  const objectId = object?.objectId;
  if (!objectId) return null;
  const connected = await cdp.send('Runtime.callFunctionOn', {
    objectId,
    functionDeclaration: 'function() { return this.isConnected; }',
    returnByValue: true,
  });
  if (connected.exceptionDetails)
    throw new Error(
      connected.exceptionDetails.exception?.description ||
        connected.exceptionDetails.text
    );
  if (connected.result?.value !== false) return { objectId };
  log(
    `[FINDING] STALE_SNAPSHOT_REF ${step.ref}; resolved node is detached; ${recovering}`
  );
  return null;
}

/** The one current node with the stale entry's role, name and frame, or the not-found result. */
async function recoverStaleObject(
  cdp: Session,
  entry: SnapshotRef,
  label: string
): Promise<Miss | { miss?: undefined; object: { objectId: string } }> {
  const candidates = (await collectRefs(cdp, {})).useful.filter(
    n =>
      n.role === entry.role &&
      n.fullName === entry.name &&
      (!entry.frame || n.frame === entry.frame)
  );
  if (candidates.length !== 1)
    return {
      miss: {
        label,
        details: {
          found: false,
          ambiguous: candidates.length > 1,
          staleRef: true,
          error: `Stale ref recovery found ${candidates.length} role/name matches; refresh the snapshot`,
        },
        objectId: null,
      },
    };
  const { object } = await cdp.send('DOM.resolveNode', {
    backendNodeId: candidates[0].backendDOMNodeId,
  });
  return { object };
}

async function checkObject(
  cdp: Session,
  step: DomStep,
  objectId: string,
  cfg: DomConfig
) {
  return valueOf(
    await cdp.send('Runtime.callFunctionOn', {
      objectId,
      awaitPromise: true,
      returnByValue: true,
      functionDeclaration: `async function() {
      ${coreBodyJs(cfg.traceEvents)}
      const details = await checkElement(this, ${checkArgs(step, cfg)});
      details.ref = ${JSON.stringify(step.ref)};
      return details;
    }`,
    })
  );
}

// Returns { label, details, objectId }; objectId is the element handle for geometry, files, verify.
export async function locateAndCheck(
  cdp: Session,
  step: DomStep,
  { cfg, log }: DomContext
): Promise<LocatedStep> {
  if (!step.ref && !step.role && !step.name)
    return locateBySelector(cdp, step, cfg);
  const label = step.ref
    ? `ref:${step.ref}`
    : `role:${step.role} name:${step.name}`;
  let entry: SnapshotRef;
  if (step.ref) entry = resolveRefEntry(cdp, step.ref);
  else {
    const found = await entryByRoleName(cdp, step, label);
    if (found.miss) return found.miss;
    entry = found.entry;
  }
  const live = await liveObject(cdp, step, entry, log);
  let object = live;
  if (!object) {
    const recovery = await recoverStaleObject(cdp, entry, label);
    if (recovery.miss) return recovery.miss;
    object = recovery.object;
  }
  const details = await checkObject(cdp, step, object.objectId, cfg);
  if (!live) details.recoveredFromStaleRef = true;
  return { label, details, objectId: object.objectId };
}

function readyToReport(step: DomStep, d: StepDetails) {
  if (step.action === 'inspect' || d.ambiguous || d.staleRef) return true;
  if (step.action === 'wait')
    return Boolean(d.found && d.visible && d.waitTextMatches);
  return Boolean(
    d.acted ||
    d.pending ||
    (d.found && d.operation !== 'blocked-by-actionability')
  );
}

/** Re-checks the target until it can be reported or acted on, or the wait deadline passes. */
export async function locateWhenReady(
  cdp: Session,
  step: DomStep,
  ctx: DomContext
): Promise<LocatedStep> {
  const start = Date.now();
  let result: LocatedStep;
  let lastProgress = -Infinity;
  do {
    result = await locateAndCheck(cdp, step, ctx);
    const d = result.details;
    if (readyToReport(step, d)) return result;
    if (Date.now() - start - lastProgress >= 1000) {
      lastProgress = Date.now() - start;
      ctx.log(
        `[PROGRESS] waiting target=${JSON.stringify(result.label)} elapsedMs=${lastProgress} found=${Boolean(d.found)} visible=${Boolean(d.visible)} disabled=${Boolean(d.disabled)}`
      );
    }
    await sleep(150);
  } while (Date.now() - start < ctx.cfg.waitMs);
  return result;
}
