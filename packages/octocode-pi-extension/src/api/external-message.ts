import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { fencedBody } from '../shared/fence.js';
import { formatClock } from '../shared/format.js';
import { EXTERNAL_MESSAGE_TYPE } from '../shared/messages.js';
import { clip, resultBlock } from '../shared/render.js';
import { contentText, isRecord } from '../shared/util.js';

/** `✉ from <client>`, then the client's text: untrusted, so resultBlock sanitizes and previews it instead of Markdown. */
export function registerExternalMessageRenderer(pi: ExtensionAPI): void {
  pi.registerMessageRenderer(EXTERNAL_MESSAGE_TYPE, (message, { expanded, outputPad }, theme) => {
    const details = isRecord(message.details) ? message.details : {};
    const from = clip(typeof details['from'] === 'string' ? details['from'] : 'API client', 60);
    const text = contentText(message.content);
    // The content is the fenced message the model reads; the header above says who sent it, so draw only the body.
    const body = fencedBody(text);
    const box = new Box(outputPad ?? 1, 0);
    box.addChild(new Text(`${theme.fg('accent', '✉')} ${theme.fg('toolTitle', theme.bold(`from ${from}`))} ${theme.fg('dim', `· Octocode API · ${formatClock(typeof details['at'] === 'number' ? details['at'] : Date.now())}`)}`, 0, 0));
    box.addChild(resultBlock(theme, { expanded }, { summary: '', body, error: false }));
    return box;
  });
}
