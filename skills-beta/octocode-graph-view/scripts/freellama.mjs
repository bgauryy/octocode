// OCR through FreeLlama-managed Ollama: the sidecar at FREELLAMA_SERVE_ENDPOINT (default http://127.0.0.1:11435)
// owns admission, host-pressure holds and model affinity (one session per run). The raw Ollama host is used only
// when no sidecar answers; the skill never starts or stops Ollama (the operator owns its lifecycle).
const trim = (u) => u.replace(/\/$/, '');
export const serveEndpoint = () => trim(process.env.FREELLAMA_SERVE_ENDPOINT ?? 'http://127.0.0.1:11435');
export const ollamaHost = () => trim(process.env.OLLAMA_HOST?.replace(/^(?!http)/, 'http://') ?? 'http://localhost:11434');

async function call(url, init = {}, ms = 10_000) {
  const res = await fetch(url, { ...init, headers: { 'content-type': 'application/json', ...init.headers }, signal: AbortSignal.timeout(ms) });
  const text = await res.text();
  let body;
  try { body = JSON.parse(text); } catch { body = { error: text.slice(0, 300) }; }
  return { ok: res.ok, status: res.status, body };
}

// Installed model names from the sidecar catalog or a raw /api/tags answer (both shapes tolerated).
export function modelNames(body) {
  const list = Array.isArray(body) ? body : body?.models ?? body?.installed ?? body?.catalog ?? [];
  return list.map((m) => (typeof m === 'string' ? m : m?.name ?? m?.model ?? m?.tag)).filter(Boolean);
}

// The installed tag a model name denotes (`glm-ocr` -> `glm-ocr:latest`); managed tasks need the exact tag.
export const installedTag = (models, model) => models.find((n) => n === model) ?? models.find((n) => n.startsWith(`${model}:`)) ?? null;

// Where OCR runs: {via:'freellama'|'ollama', endpoint, model (exact tag), models} or {via:'none', endpoint, reason}.
export async function ocrRoute(model = 'glm-ocr') {
  const fl = serveEndpoint();
  const health = await call(`${fl}/_freellama/v1/health`, {}, 3000).catch(() => null);
  if (health?.ok) {
    const catalog = await call(`${fl}/_freellama/v1/models`, {}, 10_000).catch((e) => ({ ok: false, body: { error: e.message } }));
    if (!catalog.ok) return { via: 'none', endpoint: fl, reason: `freellama at ${fl} cannot reach Ollama (${String(catalog.body?.error ?? 'no response').slice(0, 120)}); start Ollama, freellama does not start it` };
    const models = modelNames(catalog.body);
    const tag = installedTag(models, model);
    if (!tag) return { via: 'none', endpoint: fl, reason: `${model} is not installed (freellama lists: ${models.join(', ') || 'nothing'})`, models };
    return { via: 'freellama', endpoint: fl, model: tag, models };
  }
  const host = ollamaHost();
  const tags = await call(`${host}/api/tags`, {}, 3000).catch(() => null);
  if (!tags?.ok) return { via: 'none', endpoint: host, reason: `neither freellama at ${fl} nor Ollama at ${host} is reachable` };
  const models = modelNames(tags.body);
  const tag = installedTag(models, model);
  if (!tag) return { via: 'none', endpoint: host, reason: `${model} is not installed`, models };
  return { via: 'ollama', endpoint: host, model: tag, models };
}

// Model-affinity session for one run's related OCR tasks (no prompt or KV storage); null when the sidecar refuses.
export async function createSession(endpoint) {
  try {
    const r = await call(`${endpoint}/_freellama/v1/sessions`, { method: 'POST', body: '{}' }, 5000);
    return r.ok ? r.body.session_id ?? null : null;
  } catch { return null; }
}
export async function deleteSession(endpoint, sessionId) {
  if (!sessionId) return;
  try { await fetch(`${endpoint}/_freellama/v1/sessions/${sessionId}`, { method: 'DELETE', signal: AbortSignal.timeout(5000) }); } catch { /* the sidecar expires idle sessions itself */ }
}

// The managed vision task for one image (base64, no data-URI prefix). Decoding options stay Ollama-shaped.
// priority: 'interactive' for the read the brief depends on, 'background' for strips and crops.
// OCR prompts are about 2K tokens at most (image plus a few hundred output tokens), so a 4K context keeps the
// runner's KV cache small (measured on glm-ocr: 2.91 GB at Ollama's 16K default, 2.00 GB at 4K, same speed).
export const OCR_CONTEXT = 4096;
export function visionRequest({ model, prompt, image, maxTokens, sessionId, options = {}, priority = 'background', contextTokens = OCR_CONTEXT }) {
  return {
    task: 'vision',
    objective: 'fastest',
    model,
    prompt,
    images: [image],
    required_capabilities: ['vision'],
    ...(sessionId ? { session_id: sessionId } : {}),
    ...(contextTokens ? { context_tokens: contextTokens } : {}),
    priority,
    request_options: { options: { temperature: 0, num_predict: maxTokens, ...options } },
  };
}

// The generated text of a managed-task receipt or a raw /api/chat answer.
export function textOf(body) {
  const r = body?.response ?? body?.result ?? body;
  return r?.message?.content ?? r?.response ?? r?.content ?? r?.text ?? null;
}
export const evalCount = (body) => body?.response?.eval_count ?? body?.eval_count ?? body?.metrics?.eval_count ?? body?.usage?.output_tokens;
export const errorOf = (body) => (typeof body?.error === 'string' ? body.error : body?.error?.message ?? body?.error?.code ?? (body?.error ? JSON.stringify(body.error).slice(0, 200) : null));

// Load the runner through managed admission before the first OCR, so the first read does not pay the load.
export async function warm(route) {
  if (route.via !== 'freellama') return null;
  try {
    // Warm at the OCR context: a runner warmed at Ollama's default would reload on the first 4K task.
    const r = await call(`${route.endpoint}/_freellama/v1/warm`, { method: 'POST', body: JSON.stringify({ model: route.model, context_tokens: OCR_CONTEXT }) }, 120_000);
    return r.ok ? { ms: r.body?.execution?.load_ms ?? null, resident: r.body?.admission?.mode === 'resident_shared' } : null;
  } catch { return null; }
}

// Token budget for reading `labels` expected labels: glm-ocr emits about 6 tokens per short label and starts
// looping soon after the visible text ends (measured: 31 labels read in 200 tokens; at 300 and 600 Ollama aborts
// with "token repeat limit reached" after ~6 s). Panel strips need up to ~250 tokens; 400 keeps a margin.
export const STRIP_TOKENS = 400;
export const labelTokens = (labels) => Math.min(600, Math.max(48, 6 * labels + 12));

// One OCR call; returns {ok, status, text, tokens, error, body}.
export async function ocr(route, { model, prompt, image, maxTokens, sessionId, options = {}, priority }, ms = 600_000) {
  if (route.via === 'freellama') {
    const r = await call(`${route.endpoint}/_freellama/v1/tasks`, { method: 'POST', body: JSON.stringify(visionRequest({ model, prompt, image, maxTokens, sessionId, options, priority })) }, ms);
    return { ok: r.ok && !errorOf(r.body), status: r.status, text: textOf(r.body), tokens: evalCount(r.body), error: errorOf(r.body), body: r.body };
  }
  const r = await call(`${route.endpoint}/api/chat`, { method: 'POST', body: JSON.stringify({ model, stream: false, messages: [{ role: 'user', content: prompt, images: [image] }], options: { temperature: 0, num_predict: maxTokens, num_ctx: OCR_CONTEXT, ...options } }) }, ms);
  return { ok: r.ok, status: r.status, text: textOf(r.body), tokens: evalCount(r.body), error: errorOf(r.body), body: r.body };
}
