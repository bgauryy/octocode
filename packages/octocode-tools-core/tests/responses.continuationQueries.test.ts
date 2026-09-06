import { expect, it } from 'vitest';
import { cleanJsonObject } from '../src/responses.js';
import { buildResponseChannels } from '../src/utils/response/responseChannels.js';

it('preserves semantic empty values in executable queries while trimming ordinary empty output', () => {
  const query = {
    selector: {},
    options: { nested: {} },
    names: [],
    nullable: null,
  };
  const response = {
    data: {
      empty: {},
      next: { continue: { tool: 'exampleTool', query, why: 'Continue' } },
    },
  };
  const expected = {
    data: {
      next: { continue: { tool: 'exampleTool', query, why: 'Continue' } },
    },
  };
  expect(cleanJsonObject(response)).toEqual(expected);
  expect(buildResponseChannels(response, []).structuredContent).toEqual(
    expected
  );
});
