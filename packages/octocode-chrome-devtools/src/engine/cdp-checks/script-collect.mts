import type { Payload, Session } from '../cdp-connection.mjs';

/** One script Chrome parsed, with the Debugger.scriptParsed fields the recipes report. */
export type ScriptRow = {
  scriptId: string;
  url: string;
  length: number;
  hash: string;
  sourceMapURL: string;
  isModule: boolean;
  hasSourceURL: boolean;
  startLine: number;
  endLine: number;
  executionContextId: number | null;
};

/** Starts collecting every Debugger.scriptParsed (Debugger.enable replays scripts that already exist). */
export async function collectScripts(cdp: Session) {
  const scripts = new Map<string, ScriptRow>();
  cdp.on('Debugger.scriptParsed', (e: Payload) => {
    if (scripts.has(e.scriptId)) return;
    scripts.set(e.scriptId, {
      scriptId: String(e.scriptId),
      url: typeof e.url === 'string' ? e.url : '',
      length: Number(e.length ?? 0),
      hash: String(e.hash ?? ''),
      sourceMapURL: typeof e.sourceMapURL === 'string' ? e.sourceMapURL : '',
      isModule: Boolean(e.isModule),
      hasSourceURL: Boolean(e.hasSourceURL),
      startLine: Number(e.startLine ?? 0),
      endLine: Number(e.endLine ?? 0),
      executionContextId:
        typeof e.executionContextId === 'number' ? e.executionContextId : null,
    });
  });
  await cdp.send('Debugger.enable', {});
  await cdp.send('Debugger.setSkipAllPauses', { skip: true });
  return scripts;
}

/** Navigates (or reloads) after the listeners attach and waits for load, bounded, then a short settle. */
export async function loadAndSettle(
  cdp: Session,
  action: { url?: string; reload?: boolean },
  scripts: Map<string, ScriptRow>,
  timeoutMs = 15000
) {
  await cdp.send('Page.enable');
  const loaded = new Promise<void>(resolve => {
    const t = setTimeout(resolve, timeoutMs);
    cdp.on('Page.loadEventFired', () => {
      clearTimeout(t);
      resolve();
    });
  });
  // Scripts replayed by Debugger.enable belong to the document this load replaces.
  if (action.url || action.reload) scripts.clear();
  if (action.url) await cdp.send('Page.navigate', { url: action.url });
  else if (action.reload) await cdp.send('Page.reload', { ignoreCache: true });
  else return;
  await loaded;
  await new Promise(r => setTimeout(r, 300));
}

/** The document URLs of every frame: a script reported under one of these is inline in that document. */
export async function documentUrls(cdp: Session) {
  const urls = new Set<string>();
  try {
    const walk = (node: Payload) => {
      if (node.frame?.url) urls.add(String(node.frame.url));
      for (const child of node.childFrames ?? []) walk(child);
    };
    walk((await cdp.send('Page.getFrameTree')).frameTree ?? {});
  } catch {
    // No frame tree (a non-page target): only url-less scripts count as inline.
  }
  return urls;
}
