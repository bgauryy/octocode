// Runs the `helper` MCP tool (full preset) against real fixtures in a throwaway workspace.
import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fixture, data } from './agent-fixture.mjs';

export const TMP = '.octocode/octocode-chrome-devtools/tmp';

/** One fixture per test: `{ call, put, read, cwd }`; `call(name, args)` returns { result, data, text }. */
export const withHelpers = fn =>
  fixture(
    async ({ client, cwd }) => {
      const call = async (name, args) => {
        const result = await client.callTool({
          name: 'helper',
          arguments: { name, args },
        });
        const text = result.content?.find(row => row.type === 'text')?.text;
        return {
          result,
          text,
          data: result.isError ? undefined : data(result).data,
          error: result.isError ? text : undefined,
        };
      };
      const put = (name, value) => {
        const file = join(cwd, name);
        mkdirSync(dirname(file), { recursive: true });
        writeFileSync(
          file,
          typeof value === 'string' ? value : JSON.stringify(value, null, 2)
        );
        return file;
      };
      const read = name => readFileSync(join(cwd, name), 'utf8');
      await fn({
        call,
        put,
        read,
        cwd,
        exists: name => existsSync(join(cwd, name)),
      });
    },
    { full: true }
  );

/** A HAR 1.2 entry; secrets sit in the headers, cookies, query and bodies. */
export const harEntry = ({ url, status, headers = [], body, post }) => ({
  startedDateTime: '2024-01-01T00:00:00Z',
  time: 10,
  request: {
    method: post ? 'POST' : 'GET',
    url,
    httpVersion: 'h2',
    headers,
    cookies: [],
    queryString: [],
    headersSize: -1,
    bodySize: post ? post.length : 0,
    ...(post ? { postData: { mimeType: 'application/json', text: post } } : {}),
  },
  response: {
    status,
    statusText: '',
    httpVersion: 'h2',
    headers: [],
    cookies: [],
    content: {
      size: body?.length ?? 0,
      mimeType: 'application/json',
      text: body,
    },
    redirectURL: '',
    headersSize: -1,
    bodySize: body?.length ?? 0,
  },
  cache: {},
  timings: { send: 0, wait: 10, receive: 0 },
});
export const har = (...entries) => ({
  log: { version: '1.2', creator: { name: 'test', version: '1' }, entries },
});
