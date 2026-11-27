/**
 * Page HTML: PAGEHTML_URL, PAGEHTML_SELECTOR, PAGEHTML_FRAMES (1/0), PAGEHTML_SHADOW (1/0).
 * Writes page.html (whole document, doctype included), frame-<n>.html per other frame,
 * selector-<n>.html per match and html-index.json. HTML stays in files; stdout carries counts.
 */
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import {
  attachFrameSessions,
  listFrames,
  type FrameRef,
} from './evidence-frames.mjs';
import {
  navigateAndWait,
  out,
  saveEvidence,
  saveIndex,
  showRows,
} from './evidence-io.mjs';
import { flagOption, textOption } from './stream-options.mjs';

type HtmlRow = {
  file: string;
  url: string;
  frame: string;
  bytes?: number;
  sha256?: string;
  selector?: string;
  shadowRoots?: number;
  error?: string;
};

// Serializes a document with every open shadow root (declarative shadow DOM form), doctype first.
const DOCUMENT_HTML = `function (doc) {
  const roots = [];
  const walk = root => {
    for (const el of root.querySelectorAll("*"))
      if (el.shadowRoot) { roots.push(el.shadowRoot); walk(el.shadowRoot); }
  };
  walk(doc);
  const doctype = doc.doctype ? new XMLSerializer().serializeToString(doc.doctype) : "";
  return { html: doctype + doc.documentElement.getHTML({ serializableShadowRoots: true, shadowRoots: roots }), roots: roots.length };
}`;

/** Runs the in-page serializer on the document of `sessionId` (the page session when undefined). */
async function shadowDocumentHtml(cdp: Session, sessionId?: string) {
  const { result, exceptionDetails } = await cdp.send(
    'Runtime.evaluate',
    {
      expression: `(${DOCUMENT_HTML})(document)`,
      returnByValue: true,
    },
    sessionId
  );
  if (exceptionDetails)
    throw new Error(
      `element.getHTML is unavailable in this Chrome: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`
    );
  return result.value as { html: string; roots: number };
}

async function documentHtml(
  cdp: Session,
  frame: FrameRef,
  shadow: boolean
): Promise<{ html: string; roots?: number }> {
  const { sessionId } = frame;
  if (shadow && (frame.main || sessionId)) {
    const { html, roots } = await shadowDocumentHtml(cdp, sessionId);
    return { html, roots };
  }
  if (frame.main || sessionId) {
    const { root } = await cdp.send(
      'DOM.getDocument',
      { depth: 0, pierce: false },
      sessionId
    );
    const { outerHTML } = await cdp.send(
      'DOM.getOuterHTML',
      { nodeId: root.nodeId },
      sessionId
    );
    return { html: outerHTML };
  }
  throw new Error('frame has no session');
}

/** A same-process child frame's document: the frame owner's contentDocument. */
async function childFrameHtml(
  cdp: Session,
  frame: FrameRef,
  shadow: boolean
): Promise<{ html: string; roots?: number }> {
  await cdp.send('DOM.getDocument', { depth: 0 }, frame.sessionId);
  const owner = await cdp.send(
    'DOM.getFrameOwner',
    { frameId: frame.frameId },
    frame.sessionId
  );
  const { node } = await cdp.send(
    'DOM.describeNode',
    { backendNodeId: owner.backendNodeId, depth: 1, pierce: true },
    frame.sessionId
  );
  const content = node.contentDocument;
  if (!content?.backendNodeId)
    throw new Error('frame document is not reachable from its owner');
  if (shadow) {
    const { object } = await cdp.send(
      'DOM.resolveNode',
      { backendNodeId: content.backendNodeId },
      frame.sessionId
    );
    const { result, exceptionDetails } = await cdp.send(
      'Runtime.callFunctionOn',
      {
        objectId: object.objectId,
        functionDeclaration: `function () { return (${DOCUMENT_HTML})(this); }`,
        returnByValue: true,
      },
      frame.sessionId
    );
    if (exceptionDetails) throw new Error(exceptionDetails.text);
    return result.value;
  }
  const { outerHTML } = await cdp.send(
    'DOM.getOuterHTML',
    { backendNodeId: content.backendNodeId },
    frame.sessionId
  );
  return { html: outerHTML };
}

export async function run(cdp: Session) {
  const url = textOption('PAGEHTML_URL');
  const selector = textOption('PAGEHTML_SELECTOR');
  const frames = flagOption('PAGEHTML_FRAMES', false);
  const shadow = flagOption('PAGEHTML_SHADOW', false);
  const dir = cdp.outputDir;
  const rows: HtmlRow[] = [];
  const failures: { file: string; frame: string; error: string }[] = [];

  const fail = (row: HtmlRow, error: unknown) => {
    row.error = errorMessage(error);
    failures.push({ file: row.file, frame: row.frame, error: row.error });
  };

  const attached = frames ? await attachFrameSessions(cdp, ['Page']) : null;
  if (url) {
    const nav = await navigateAndWait(cdp, url);
    if (nav.errorText) {
      out(
        `[FINDING] PAGE_HTML_NAVIGATION_FAILED ${JSON.stringify({ url, error: nav.errorText })}`
      );
      process.exitCode = 1;
      return;
    }
  }
  await attached?.settle();
  const tree: FrameRef[] = await listFrames(
    cdp,
    attached?.sessions ?? new Map()
  );
  const main = tree.find(f => f.main) as FrameRef;

  // page.html: the document of the main frame.
  const pageRow: HtmlRow = {
    file: 'page.html',
    url: main.url,
    frame: main.frameId,
  };
  try {
    const { html, roots } = await documentHtml(cdp, main, shadow);
    Object.assign(pageRow, saveEvidence(dir, pageRow.file, html));
    if (shadow) pageRow.shadowRoots = roots;
  } catch (error) {
    fail(pageRow, error);
  }
  rows.push(pageRow);

  // frame-<n>.html: every other frame.
  if (frames) {
    let n = 0;
    for (const frame of tree.filter(f => !f.main)) {
      n++;
      const row: HtmlRow = {
        file: `frame-${n}.html`,
        url: frame.url,
        frame: frame.frameId,
      };
      try {
        const { html, roots } = frame.sessionId
          ? await documentHtml(cdp, { ...frame, main: false }, shadow)
          : await childFrameHtml(cdp, frame, shadow);
        Object.assign(row, saveEvidence(dir, row.file, html));
        if (shadow) row.shadowRoots = roots;
      } catch (error) {
        fail(row, error);
      }
      rows.push(row);
    }
  }
  await attached?.stop();

  // selector-<n>.html: one file per match.
  let matches = 0;
  if (selector) {
    try {
      const { root } = await cdp.send('DOM.getDocument', {
        depth: -1,
        pierce: shadow,
      });
      const found: Payload = await cdp.send('DOM.querySelectorAll', {
        nodeId: root.nodeId,
        selector,
      });
      for (const nodeId of found.nodeIds ?? []) {
        matches++;
        const row: HtmlRow = {
          file: `selector-${matches}.html`,
          url: main.url,
          frame: main.frameId,
          selector,
        };
        try {
          const { outerHTML } = await cdp.send('DOM.getOuterHTML', { nodeId });
          Object.assign(row, saveEvidence(dir, row.file, outerHTML));
        } catch (error) {
          fail(row, error);
        }
        rows.push(row);
      }
      if (!matches) {
        out(
          `[FINDING] PAGE_HTML_NO_MATCH ${JSON.stringify({ selector, url: main.url })}`
        );
        process.exitCode = 1;
      }
    } catch (error) {
      out(
        `[FINDING] PAGE_HTML_SELECTOR_FAILED ${JSON.stringify({ selector, error: errorMessage(error) })}`
      );
      process.exitCode = 1;
    }
  }

  const index = saveIndex(dir, 'html-index.json', rows);
  const saved = rows.filter(r => r.file && !r.error);
  out(
    `[METRIC] PAGE_HTML files=${saved.length} frames=${rows.filter(r => r.file.startsWith('frame-')).length} matches=${matches} bytes=${saved.reduce((s, r) => s + (r.bytes ?? 0), 0)} shadow=${shadow}`
  );
  showRows(
    rows,
    r =>
      `[HTML] ${r.file} ${r.error ? `unreadable ${r.error}` : `${r.bytes} bytes`} ${r.url}${r.selector ? ` selector=${r.selector}` : ''}`,
    'HTML',
    index
  );
  for (const row of rows.filter(r => r.error))
    out(`[FINDING] PAGE_HTML_UNAVAILABLE ${JSON.stringify(row)}`);
  out(`[ARTIFACT] PAGE_HTML ${index}`);
  for (const row of saved) out(`[ARTIFACT] HTML_FILE ${dir}/${row.file}`);
  if (failures.length) process.exitCode = 1;
}
