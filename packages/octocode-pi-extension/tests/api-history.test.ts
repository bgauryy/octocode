import { describe, expect, it } from 'vitest';
import { historyPage } from '../src/api/history.js';

const message = (role: string, content: unknown) => ({ type: 'message', message: { role, content } });

describe('historyPage', () => {
  it('pages the user and assistant text of a branch, skipping every other entry', () => {
    const branch = [
      message('user', 'first'),
      { type: 'custom', message: { role: 'user', content: 'not a message entry' } },
      message('toolResult', 'tool output'),
      message('assistant', [{ type: 'text', text: 'an ' }, { type: 'text', text: 'answer' }]),
      { type: 'message' },
      { type: 'message', message: 'not an object' },
      { type: 'message', message: { content: 'no role' } },
      message('user', ''),
      'not an entry',
      null,
      message('user', 'last'),
    ];
    expect(historyPage(branch, 'final', 20)).toEqual({
      messages: [
        { index: 0, role: 'user', text: 'first' },
        { index: 1, role: 'assistant', text: 'an answer' },
        { index: 2, role: 'user', text: 'last' },
      ],
      total: 3,
      limit: 20,
      last: 'final',
    });
  });

  it('takes the `limit` messages before `before` and names the earlier page', () => {
    const branch = ['a', 'b', 'c', 'd'].map((text) => message('user', text));
    expect(historyPage(branch, '', 2, 3)).toEqual({ messages: [{ index: 1, role: 'user', text: 'b' }, { index: 2, role: 'user', text: 'c' }], total: 4, limit: 2, earlier: { before: 1 }, last: null });
  });
});
