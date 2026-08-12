import { writeFileSync } from 'fs';
import { join } from 'path';

// webmcp intent: discover and optionally invoke tools a page declared via
// document.modelContext.registerTool(...), using the CDP WebMCP domain
// instead of DOM selectors/clicks. Requires Chrome launched with
// --enableFeatures WebMCP (see open-browser.mjs) — most pages will not have
// registered any tools yet, so WEBMCP_NO_TOOLS is an expected, non-error
// outcome, not a failure.
//
// Env:
//   WEBMCP_ACTION   'list' (default) | 'invoke'
//   WEBMCP_TOOL     tool name, required for invoke
//   WEBMCP_INPUT    JSON string matching the tool's inputSchema, required for invoke
//   WEBMCP_FRAME    frameId, only needed if WEBMCP_TOOL name collides across frames
//   WEBMCP_WAIT_MS  discovery window in ms (default 2500), only spent in full when a tool was found

const ACTION = process.env.WEBMCP_ACTION ?? 'list';
const TOOL_NAME = process.env.WEBMCP_TOOL ?? '';
const RAW_INPUT = process.env.WEBMCP_INPUT ?? '{}';
const WANT_FRAME = process.env.WEBMCP_FRAME ?? '';
const WAIT_MS = Number.parseInt(process.env.WEBMCP_WAIT_MS ?? '2500', 10);
const SETTLE_MS = Math.min(400, WAIT_MS);
const INVOKE_TIMEOUT_MS = 10000;

function assertAllowedAction(action) {
  if (!['list', 'invoke'].includes(action)) {
    throw new Error(`Unsupported WEBMCP_ACTION=${action}. Use list or invoke.`);
  }
}

// Unannotated or ambiguous tools default to the riskier label — the Mutation
// Gate only means something if "unknown" is treated as "assume it mutates".
function toolRisk(tool) {
  return tool.annotations?.readOnly === true ? 'read-only' : 'mutating';
}

function toolKey(frameId, name) {
  return `${frameId}::${name}`;
}

export async function run(cdp) {
  assertAllowedAction(ACTION);
  await cdp.send('Runtime.enable');

  const tools = new Map();
  cdp.on('WebMCP.toolsAdded', ({ tools: added }) => {
    for (const tool of added ?? []) tools.set(toolKey(tool.frameId, tool.name), tool);
  });
  cdp.on('WebMCP.toolsRemoved', ({ tools: removed }) => {
    for (const tool of removed ?? []) tools.delete(toolKey(tool.frameId, tool.name));
  });

  try {
    await cdp.send('WebMCP.enable');
  } catch (error) {
    console.log(`[FINDING] WEBMCP_UNAVAILABLE ${cdp.targetInfo.url} — ${error.message}`);
    console.log('[REASON] WebMCP CDP domain not supported by this Chrome build/flags. Launch with --enableFeatures WebMCP on Chrome 150+, or fall back to the automate/scrape intents.');
    return;
  }

  cdp.addReasoningStep?.({
    step: 'webmcp-discover',
    hypothesis: 'Page may have registered document.modelContext tools',
    action: `Enabled WebMCP domain, waiting ${WAIT_MS}ms for toolsAdded`,
    result: 'pending',
    nextAction: ACTION === 'invoke' ? `invoke ${TOOL_NAME}` : 'list discovered tools',
  });

  // Real registrations happen synchronously during page/script load in every
  // case observed so far, so a short settle window covers the common
  // zero-tools case cheaply; only pay the full WAIT_MS once something was
  // actually found, in case more tools register asynchronously after that.
  await new Promise((r) => setTimeout(r, SETTLE_MS));
  if (tools.size > 0 && WAIT_MS > SETTLE_MS) {
    await new Promise((r) => setTimeout(r, WAIT_MS - SETTLE_MS));
  }

  const artifactPath = join(cdp.outputDir, 'webmcp-tools.json');
  const toolList = [...tools.values()];
  writeFileSync(artifactPath, `${JSON.stringify({ url: cdp.targetInfo.url, tools: toolList }, null, 2)}\n`, { mode: 0o600 });
  cdp.upsertResourceMap?.('webmcp-tools', {
    type: 'webmcp-tools',
    targetUrl: cdp.targetInfo.url,
    toolCount: toolList.length,
    toolNames: toolList.map((t) => t.name),
    artifactPath,
  });

  if (toolList.length === 0) {
    console.log(`[FINDING] WEBMCP_NO_TOOLS no page-declared tools found on ${cdp.targetInfo.url}`);
    console.log(`[ARTIFACT] WEBMCP_TOOLS ${artifactPath}`);
    return;
  }

  if (ACTION === 'list') {
    for (const tool of toolList) {
      const risk = toolRisk(tool);
      console.log(`[WEBMCP_TOOL] name=${tool.name} risk=${risk} frameId=${tool.frameId} description=${JSON.stringify(tool.description)}`);
      console.log(`[METRIC] WEBMCP_TOOL_SCHEMA name=${tool.name} inputSchema=${JSON.stringify(tool.inputSchema ?? {})}`);
    }
    console.log(`[ARTIFACT] WEBMCP_TOOLS ${artifactPath}`);
    return;
  }

  // ACTION === 'invoke'
  const matches = toolList.filter((t) => t.name === TOOL_NAME);
  if (matches.length === 0) {
    console.log(`[FINDING] WEBMCP_TOOL_NOT_FOUND name=${TOOL_NAME} available=${JSON.stringify(toolList.map((t) => t.name))}`);
    return;
  }
  let tool = matches[0];
  if (matches.length > 1) {
    const disambiguated = WANT_FRAME ? matches.find((t) => t.frameId === WANT_FRAME) : null;
    if (!disambiguated) {
      console.log(`[FINDING] WEBMCP_TOOL_AMBIGUOUS name=${TOOL_NAME} frames=${JSON.stringify(matches.map((t) => t.frameId))} — set WEBMCP_FRAME to one of these to disambiguate`);
      return;
    }
    tool = disambiguated;
  }

  let input;
  try {
    input = JSON.parse(RAW_INPUT);
  } catch (error) {
    console.log(`[FINDING] WEBMCP_INVALID_INPUT WEBMCP_INPUT is not valid JSON: ${error.message}`);
    return;
  }

  const risk = toolRisk(tool);
  console.log(`[REASON] Invoking WebMCP tool "${TOOL_NAME}" (risk=${risk}) with explicit input — page code runs with page privileges, same trust boundary as a real click.`);

  // Buffer every toolResponded event from before the invoke call even goes
  // out, so a response that arrives faster than we can attach a filtered
  // listener afterward still gets caught instead of causing a false Timeout.
  const responseBuffer = new Map();
  const bufferResponse = (params) => responseBuffer.set(params.invocationId, params);
  cdp.on('WebMCP.toolResponded', bufferResponse);

  const { invocationId } = await cdp.send('WebMCP.invokeTool', {
    frameId: tool.frameId,
    toolName: TOOL_NAME,
    input,
  });
  console.log(`[ACTION] invoked WebMCP tool ${TOOL_NAME} invocationId=${invocationId}`);

  const responded = await new Promise((resolve) => {
    const buffered = responseBuffer.get(invocationId);
    if (buffered) {
      cdp.off('WebMCP.toolResponded', bufferResponse);
      resolve(buffered);
      return;
    }
    const timer = setTimeout(() => {
      cdp.off('WebMCP.toolResponded', bufferResponse);
      cdp.off('WebMCP.toolResponded', liveHandler);
      cdp.send('WebMCP.cancelInvocation', { invocationId }).catch(() => {});
      resolve({ status: 'Timeout', invocationId });
    }, INVOKE_TIMEOUT_MS);
    const liveHandler = (params) => {
      if (params.invocationId !== invocationId) return;
      clearTimeout(timer);
      cdp.off('WebMCP.toolResponded', bufferResponse);
      cdp.off('WebMCP.toolResponded', liveHandler);
      resolve(params);
    };
    cdp.on('WebMCP.toolResponded', liveHandler);
  });

  const resultPath = join(cdp.outputDir, 'webmcp-invocation.json');
  writeFileSync(resultPath, `${JSON.stringify(responded, null, 2)}\n`, { mode: 0o600 });

  console.log(`[WEBMCP_RESULT] tool=${TOOL_NAME} status=${responded.status} output=${JSON.stringify(responded.output ?? null)}`);
  if (responded.status === 'Error') console.log(`[FINDING] WEBMCP_INVOCATION_ERROR ${responded.errorText ?? 'unknown error'}`);
  if (responded.status === 'Timeout') console.log(`[FINDING] WEBMCP_INVOCATION_TIMEOUT no toolResponded within ${INVOKE_TIMEOUT_MS}ms`);
  console.log(`[ARTIFACT] WEBMCP_INVOCATION ${resultPath}`);
}
