# Output

Deliver task content only. Never append probe output or generated process metadata.

Use Markdown for findings and links. Preserve native browser captures in their returned formats, such as JSON and images.

## Response

Supported finding, source URL, capture path, observed post-action state, and any coverage gap.

## Saved result

Keep captures in the browser runtime location (normally `<workspace>/.octocode/tmp/chrome-devtools/`); link to them instead of pasting large payloads.
