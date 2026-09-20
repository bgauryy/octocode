import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';

const DEFAULT_BASE_URL = 'https://api.typesafe.ai';
const DEFAULT_MODEL = 'jev-latest';
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
const MAX_RESOURCE_CHARS = 80_000;
const MAX_RESOURCES = 25;
const MAX_QUESTIONS = 5;

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
  if (entries.length > MAX_QUESTIONS) {
    fail(`questions supports at most ${MAX_QUESTIONS} entries.`);
  }
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
  const chars = [...serialized].length;
  if (chars > MAX_RESOURCE_CHARS) {
    fail(
      `resource ${JSON.stringify(id)} is ${chars} characters; ` +
        `split it into pages of at most ${MAX_RESOURCE_CHARS} characters`,
    );
  }
  return {
    provider: { id, content },
    receipt: {
      id,
      ...source,
      chars,
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
  const state = spec.state;
  let resources = [];
  if (hasResources) {
    if (spec.resources.length === 0) fail('resources must not be empty.');
    if (spec.resources.length > MAX_RESOURCES) {
      fail(`resources supports at most ${MAX_RESOURCES} entries.`);
    }
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
  }

  const model = spec.model ?? env.OCTOCODE_JEV_MODEL ?? DEFAULT_MODEL;
  if (typeof model !== 'string' || !model.trim()) fail('model must be non-blank.');
  const resourceMode = hasResources ? (spec.resourceMode ?? 'matrix') : 'direct';
  if (hasResources && !['matrix', 'combined'].includes(resourceMode)) {
    fail('resourceMode must be matrix or combined.');
  }
  const requests = hasResources
    ? resourceMode === 'matrix'
      ? resources.map(({ provider }) => ({
          resourceId: provider.id,
          body: {
            model: model.trim(),
            state: { resource: provider },
            questions: spec.questions,
          },
        }))
      : [
          {
            body: {
              model: model.trim(),
              state: { resources: resources.map(({ provider }) => provider) },
              questions: spec.questions,
            },
          },
        ]
    : [{ body: { model: model.trim(), state, questions: spec.questions } }];
  return {
    requests,
    receipt: {
      requestedModel: model.trim(),
      mode: resourceMode,
      questionCount: Object.keys(spec.questions).length,
      resourceCount: resources.length,
      logicalCells:
        (resources.length || 1) * Object.keys(spec.questions).length,
      providerCallsPerPass: requests.length,
      resources: resources.map(({ receipt }) => receipt),
      requestBytesPerPass: requests.reduce(
        (sum, request) => sum + Buffer.byteLength(JSON.stringify(request.body)),
        0,
      ),
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
  const usage = successful.map(sample => sample.response?.usage);
  const completeUsage = usage.filter(
    row =>
      row &&
      Number.isFinite(row.input_tokens) &&
      Number.isFinite(row.output_tokens),
  );
  const tokenTotal = field =>
    successful.length > 0 &&
    usage.every(row => row && Number.isFinite(row[field]))
      ? usage.reduce((sum, row) => sum + row[field], 0)
      : null;
  const resolvedModels = [
    ...new Set(
      successful
        .map(sample => sample.response?.model)
        .filter(model => typeof model === 'string' && model.trim()),
    ),
  ];
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
      inputTokens: tokenTotal('input_tokens'),
      outputTokens: tokenTotal('output_tokens'),
      reportedSamples: completeUsage.length,
      missingSamples: successful.length - completeUsage.length,
    },
    resolvedModels,
  };
}

export async function runExperiment(prepared, options = {}) {
  const repeat = positiveInteger(options.repeat, 'repeat', 1);
  const tasks = prepared.requests.flatMap(request =>
    Array.from({ length: repeat }, (_, iteration) => ({ ...request, iteration })),
  );
  const concurrency = Math.min(positiveInteger(options.concurrency, 'concurrency', 1), tasks.length);
  const samples = new Array(tasks.length);
  let nextIndex = 0;
  async function worker() {
    while (nextIndex < tasks.length) {
      const index = nextIndex++;
      const task = tasks[index];
      samples[index] = {
        ...(task.resourceId ? { resourceId: task.resourceId } : {}),
        iteration: task.iteration,
        ...(await sendJev(task.body, options)),
      };
    }
  }
  await Promise.all(Array.from({ length: concurrency }, () => worker()));
  return {
    request: {
      ...prepared.receipt,
      repeat,
      concurrency,
      providerCalls: tasks.length,
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
