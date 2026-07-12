import { describe, expect, it } from 'vitest';

import { ToolErrors } from '../../src/errors/errorFactories.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';

describe('ToolErrors.binaryFileUnsupported', () => {
  it('keeps the binary error code and a redacted basename in the message', () => {
    const err = ToolErrors.binaryFileUnsupported('/Applications/x/app.asar');
    expect(err.errorCode).toBe(
      LOCAL_TOOL_ERROR_CODES.BINARY_FILE_UNSUPPORTED
    );
    expect(err.message).toContain('Binary file unsupported');
    expect(err.message).toContain('app.asar');
  });

  it('points the agent to a recovery tool instead of dead-ending', () => {
    // Convention: recovery guidance lives in the message, like fileAccessFailed
    // ("Use localViewStructure instead", "...using localFindFiles"). A binary is
    // not a dead end — localSearchCode greps embedded strings.
    const err = ToolErrors.binaryFileUnsupported('/x/codex');
    expect(err.message).toContain('localSearchCode');
  });
});
