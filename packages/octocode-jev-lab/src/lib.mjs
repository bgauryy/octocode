import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';

const DEFAULT_BASE_URL = 'https://api.typesafe.ai';
const DEFAULT_MODEL = 'jev-latest';
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
const MAX_RESOURCE_CHARS = 80_000;

function fail(message) {
  throw new Error(message);
}

function positiveInteger(value, name, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) {
    fail(`${name} must be a positive integer.`);
  }
  return parsed;
}

function percentile(sorted, quantile) {
  if (sorted.length === 0) return null;
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function validateQuestions(questions) {
  if (!questions || typeof questions !== 'object' || Array.isArray(questions)) {
    fail('questions must be a non-empty object keyed by question id.');
  }
  const entries = Object.entries(questions);
  if (entries.length === 0) fail('questions must not be empty.');
  for (const [id, question] of entries) {
    if (!id.trim()) fail('question ids must be non-blank.');
    if (!question || typeof question !== 'object' || Array.isArray(question)) {
      fail(`question ${JSON.stringify(id)} must be an object.`);
    }
    if (!['noul', 'choice', 'score'].includes(question.type)) {
      fail(`question ${JSON.stringify(id)} has an unsupported type.`);
    }
  }
}

async function materializeResource(resource, index, baseDirectory) {
  if (!resource || typeof resource !== 'object' || Array.isArray(resource)) {
    fail(`resources[${index}] must be an object.`);
  }
  const id = typeof resource.id === 'string' ? resource.id.trim() : '';
  if (!id) fail(`resources[${index}].id must be non-blank.`);
  const hasPath = typeof resource.path === 'string' && resource.path.trim() !== '';
  const hasValue = Object.hasOwn(resource, 'value');
  if (hasPath === hasValue) {
    fail(`resource ${JSON.stringify(id)} must provide exactly one of path or value.`);
  }

  let content;
  let source;
  if (hasPath) {
    const absolutePath = resolve(baseDirectory, resource.path);
    content = await readFile(absolutePath, 'utf8');
    source = { kind: 'file', path: relative(baseDirectory, absolutePath) };
  } else {
    content = resource.value;
    source = { kind: 'value' };
  }

  const serialized = typeof content === 'string' ? content : JSON.stringify(content);
  if (serialized === undefined) {
    fail(`resource ${JSON.stringify(id)} value must be valid JSON.`);
  }
  if (serialized.length > MAX_RESOURCE_CHARS) {
    fail(
      `resource ${JSON.stringify(id)} is ${serialized.length} characters; ` +
        `split it into pages of at most ${MAX_RESOURCE_CHARS} characters`,
    );
  }
  return {
    provider: { id, content },
    receipt: {
      id,
      ...source,
      chars: serialized.length,
      bytes: Buffer.byteLength(serialized),
      sha256: sha256(serialized),
    },
  };
}

export async function prepareExperiment(spec, inputPath, env = process.env) {
  if (!spec || typeof spec !== 'object' || Array.isArray(spec)) {
    fail('input must be a JSON object.');
  }
  validateQuestions(spec.questions);
  const hasState = Object.hasOwn(spec, 'state');
  const hasResources = Array.isArray(spec.resources);
  if (hasState === hasResources) {
    fail('provide exactly one of state or resources.');
  }

  const baseDirectory = dirname(resolve(inputPath));
  let state = spec.state;
  let resources = [];
  if (hasResources) {
    if (spec.resources.length === 0) fail('resources must not be empty.');
    const ids = new Set();
    resources = await Promise.all(
      spec.resources.map(async (resource, index) => {
        const result = await materializeResource(resource, index, baseDirectory);
        if (ids.has(result.provider.id)) {
          fail(`duplicate resource id ${JSON.stringify(result.provider.id)}.`);
        }
        ids.add(result.provider.id);
        return result;
      }),
    );
    state = { resources: resources.map(({ provider }) => provider) };
  }

  const model = spec.model ?? env.OCTOCODE_JEV_MODEL ?? DEFAULT_MODEL;
  if (typeof model !== 'string' || !model.trim()) fail('model must be non-blank.');
  const body = { model: model.trim(), state, questions: spec.questions };
  return {
    body,
    receipt: {
      model: body.model,
      questionCount: Object.keys(body.questions).length,
      resourceCount: resources.length,
      resources: resources.map(({ receipt }) => receipt),
      requestBytes: Buffer.byteLength(JSON.stringify(body)),
    },
  };
}

export function resolveEndpoint(baseUrl = DEFAULT_BASE_URL) {
  const url = new URL(baseUrl);
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (
    (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) ||
    url.username ||
    url.password ||
    !['', '/'].includes(url.pathname) ||
    url.search ||
    url.hash
  ) {
    fail('Jev base URL must be an HTTPS root; HTTP is allowed only on loopback.');
  }
  return new URL('/v1/systemone', url).toString();
}

export async function sendJev(body, options = {}) {
  const key = options.key ?? process.env.OCTOCODE_JEV_KEY;
  if (typeof key !== 'string' || !key.trim()) {
    fail('OCTOCODE_JEV_KEY is required.');
  }
  const timeoutMs = positiveInteger(options.timeoutMs, 'timeoutMs', 60_000);
  const endpoint = resolveEndpoint(
    options.baseUrl ?? process.env.OCTOCODE_JEV_BASE_URL ?? DEFAULT_BASE_URL,
  );
  const started = performance.now();
  const response = await fetch(endpoint, {
    method: 'POST',
    redirect: 'manual',
    signal: AbortSignal.timeout(timeoutMs),
    headers: {
      accept: 'application/json',
      authorization: `Bearer ${key.trim()}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  const elapsedMs = Math.round((performance.now() - started) * 100) / 100;
  if (Buffer.byteLength(text) > MAX_RESPONSE_BYTES) {
    fail('Jev response exceeded the 4 MiB safety limit.');
  }
  let providerResponse;
  try {
    providerResponse = JSON.parse(text);
  } catch {
    fail(`Jev returned non-JSON content with HTTP ${response.status}.`);
  }
  return {
    ok: response.ok,
    status: response.status,
    elapsedMs,
    response: providerResponse,
  };
}

export function summarize(samples) {
  const successful = samples.filter(sample => sample.ok);
  const durations = successful.map(sample => sample.elapsedMs).sort((a, b) => a - b);
  const usage = successful.map(sample => sample.response?.usage ?? {});
  return {
    samples: samples.length,
    successes: successful.length,
    failures: samples.length - successful.length,
    latencyMs: {
      min: durations.at(0) ?? null,
      median: percentile(durations, 0.5),
      p95: percentile(durations, 0.95),
      max: durations.at(-1) ?? null,
    },
    usage: {
      inputTokens: usage.reduce((sum, row) => sum + (row.input_tokens ?? 0), 0),
      outputTokens: usage.reduce((sum, row) => sum + (row.output_tokens ?? 0), 0),
    },
  };
}

export async function runExperiment(prepared, options = {}) {
  const repeat = positiveInteger(options.repeat, 'repeat', 1);
  const concurrency = Math.min(
    positiveInteger(options.concurrency, 'concurrency', 1),
    repeat,
  );
  const samples = new Array(repeat);
  let nextIndex = 0;
  async function worker() {
    while (nextIndex < repeat) {
      const index = nextIndex++;
      samples[index] = await sendJev(prepared.body, options);
    }
  }
  await Promise.all(Array.from({ length: concurrency }, () => worker()));
  return {
    request: {
      ...prepared.receipt,
      repeat,
      concurrency,
    },
    summary: summarize(samples),
    samples,
  };
}

export async function readExperiment(path) {
  let text;
  if (path === '-') {
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    text = Buffer.concat(chunks).toString('utf8');
  } else {
    text = await readFile(path, 'utf8');
  }
  try {
    return JSON.parse(text);
  } catch {
    fail(`cannot parse JSON input ${JSON.stringify(path)}.`);
  }
}
