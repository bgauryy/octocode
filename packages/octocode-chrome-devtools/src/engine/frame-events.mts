import { errorMessage } from './error-message.mjs';
import type {
  AttachedToTarget,
  StealthSession,
  TargetFailure,
} from './stealth-session.mjs';

// Enable event domains in related out-of-process iframes. Each target keeps its own session.
export async function observeFrameEvents(
  cdp: StealthSession,
  domains = ['Network']
) {
  // Under stealth, child targets pause until stealth resumes them with its overrides set.
  const attachParams = () =>
    cdp.stealthAutoAttach ?? {
      autoAttach: true,
      waitForDebuggerOnStart: false,
      flatten: true,
    };
  const frames = new Map<string, { targetId: string; url: string }>(),
    errors: TargetFailure[] = [],
    pending = new Set<Promise<void>>();
  const attach = ({ sessionId, targetInfo }: AttachedToTarget) => {
    if (targetInfo.type !== 'iframe') return;
    frames.set(sessionId, {
      targetId: targetInfo.targetId,
      url: targetInfo.url,
    });
    const task = (async () => {
      for (const domain of domains)
        await cdp.send(domain + '.enable', {}, sessionId);
      await cdp.send('Target.setAutoAttach', attachParams(), sessionId);
    })().catch((error: unknown) => {
      const message = errorMessage(error);
      errors.push({ targetId: targetInfo.targetId, error: message });
      console.log(
        `[FINDING] FRAME_EVENT_COVERAGE_UNAVAILABLE target=${targetInfo.targetId} ${message}`
      );
    });
    pending.add(task);
    task.finally(() => pending.delete(task));
  };
  cdp.on('Target.attachedToTarget', attach);
  try {
    await cdp.send('Target.setAutoAttach', attachParams());
  } catch (error) {
    const message = errorMessage(error);
    errors.push({ error: message });
    console.log(`[FINDING] FRAME_EVENT_COVERAGE_UNAVAILABLE ${message}`);
  }
  return {
    coverage() {
      return {
        frames: [...frames.values()],
        errors,
        initialIframeRequestsMayPrecedeAttachment: true,
      };
    },
    async stop() {
      await Promise.allSettled([...pending]);
      // Stealth keeps attaching child targets to apply its overrides.
      await cdp
        .send(
          'Target.setAutoAttach',
          cdp.stealthAutoAttach ?? {
            autoAttach: false,
            waitForDebuggerOnStart: false,
            flatten: true,
          }
        )
        .catch(() => {});
      cdp.off('Target.attachedToTarget', attach);
    },
  };
}
