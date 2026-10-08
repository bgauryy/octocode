// Enable event domains in related out-of-process iframes. Each target keeps its own session.
export async function observeFrameEvents(cdp, domains = ['Network']) {
  const frames = new Map(),
    errors = [],
    pending = new Set();
  const attach = ({ sessionId, targetInfo }) => {
    if (targetInfo.type !== 'iframe') return;
    frames.set(sessionId, {
      targetId: targetInfo.targetId,
      url: targetInfo.url,
    });
    const task = (async () => {
      for (const domain of domains)
        await cdp.send(domain + '.enable', {}, sessionId);
      await cdp.send(
        'Target.setAutoAttach',
        { autoAttach: true, waitForDebuggerOnStart: false, flatten: true },
        sessionId
      );
    })().catch(error => {
      errors.push({ targetId: targetInfo.targetId, error: error.message });
      console.log(
        `[FINDING] FRAME_EVENT_COVERAGE_UNAVAILABLE target=${targetInfo.targetId} ${error.message}`
      );
    });
    pending.add(task);
    task.finally(() => pending.delete(task));
  };
  cdp.on('Target.attachedToTarget', attach);
  try {
    await cdp.send('Target.setAutoAttach', {
      autoAttach: true,
      waitForDebuggerOnStart: false,
      flatten: true,
    });
  } catch (error) {
    errors.push({ error: error.message });
    console.log(`[FINDING] FRAME_EVENT_COVERAGE_UNAVAILABLE ${error.message}`);
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
      await cdp
        .send('Target.setAutoAttach', {
          autoAttach: false,
          waitForDebuggerOnStart: false,
          flatten: true,
        })
        .catch(() => {});
      cdp.off('Target.attachedToTarget', attach);
    },
  };
}
