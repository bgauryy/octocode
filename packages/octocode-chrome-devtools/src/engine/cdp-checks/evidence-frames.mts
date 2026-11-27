// Frames and the sessions that reach them. Same-process frames answer on the page session;
// out-of-process iframes (OOPIF) are separate targets, reached through Target auto-attach.
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import { sleep } from './stream-options.mjs';

/** A frame and the session that serves it (undefined: the page session). */
export type FrameRef = {
  frameId: string;
  parentId?: string;
  url: string;
  sessionId?: string;
  main: boolean;
};

export type FrameSessions = {
  /** targetId (= frame id) -> session id of every attached out-of-process iframe. */
  sessions: Map<string, string>;
  errors: string[];
  /** Resolves when every attached session has its domains enabled. */
  settle(): Promise<void>;
  stop(): Promise<void>;
};

/**
 * Attaches to every out-of-process iframe (recursively), enabling `domains` in each before it runs.
 * Call before navigating so early events are not missed; `onSession` runs for each new session.
 */
export async function attachFrameSessions(
  cdp: Session,
  domains: string[] = [],
  onSession?: (sessionId: string) => Promise<void> | void
): Promise<FrameSessions> {
  const sessions = new Map<string, string>();
  const errors: string[] = [];
  const pending = new Set<Promise<void>>();
  const params = {
    autoAttach: true,
    waitForDebuggerOnStart: false,
    flatten: true,
  };
  const attach = (
    { sessionId, targetInfo }: Payload,
    _meta?: unknown
  ): void => {
    if (targetInfo?.type !== 'iframe') return;
    sessions.set(targetInfo.targetId, sessionId);
    const task = (async () => {
      for (const domain of domains)
        await cdp.send(`${domain}.enable`, {}, sessionId);
      await onSession?.(sessionId);
      await cdp.send('Target.setAutoAttach', params, sessionId);
    })().catch((error: unknown) => {
      errors.push(`${targetInfo.url}: ${errorMessage(error)}`);
    });
    pending.add(task);
    void task.finally(() => pending.delete(task));
  };
  const detach = (p: Payload) => {
    for (const [targetId, id] of sessions)
      if (id === p.sessionId) sessions.delete(targetId);
  };
  cdp.on('Target.attachedToTarget', attach);
  cdp.on('Target.detachedFromTarget', detach);
  try {
    await cdp.send('Target.setAutoAttach', params);
  } catch (error) {
    errors.push(`Target.setAutoAttach: ${errorMessage(error)}`);
  }
  return {
    sessions,
    errors,
    async settle() {
      await sleep(0);
      while (pending.size) await Promise.allSettled(pending);
    },
    async stop() {
      await sleep(0);
      await Promise.allSettled(pending);
      cdp.off('Target.attachedToTarget', attach);
      cdp.off('Target.detachedFromTarget', detach);
      await cdp
        .send('Target.setAutoAttach', { ...params, autoAttach: false })
        .catch(() => {});
    },
  };
}

/** Every frame of the page: same-process frames from each session's frame tree, OOPIF through their session. */
export async function listFrames(
  cdp: Session,
  sessions: Map<string, string>
): Promise<FrameRef[]> {
  const frames: FrameRef[] = [];
  const seen = new Set<string>();
  const walkSession = async (sessionId: string | undefined, main: boolean) => {
    const { frameTree } = await cdp.send('Page.getFrameTree', {}, sessionId);
    const walk = async (node: Payload, root: boolean): Promise<void> => {
      const frame = node.frame as Payload;
      const child = sessions.get(frame.id);
      if (!root && child !== undefined && !seen.has(frame.id)) {
        seen.add(frame.id);
        await walkSession(child, false);
        return;
      }
      frames.push({
        frameId: frame.id,
        parentId: frame.parentId,
        url: frame.url + (frame.urlFragment ?? ''),
        sessionId,
        main: main && root,
      });
      for (const c of node.childFrames ?? []) await walk(c, false);
    };
    await walk(frameTree, true);
  };
  await walkSession(undefined, true);
  // The page's frame tree does not list out-of-process frames: reach each by its own session.
  for (const [targetId, sessionId] of sessions)
    if (!seen.has(targetId)) {
      seen.add(targetId);
      await walkSession(sessionId, false);
    }
  return frames;
}
