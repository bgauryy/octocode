# Media tools RFC — perception and production

Status: **implemented**

The Pi extension exposes two media tools with one clear effect boundary:

| Tool | Effect | Use it for |
|---|---|---|
| `readMedia` | Read-only | Perceive an image, inspect audio/video metadata, or produce a temporary visual representation for the model |
| `media` | Workspace write | Author an image/PDF or transform media into a requested output file |

There are exactly two media names, one routing decision, and no read operation hidden inside
the write tool.

## Decision

Tool choice follows the user's intended effect:

```text
Need to understand existing media?  -> readMedia
Need to create or transform output? -> media
```

The public discriminant is always `type`. File perception uses `path`; file transformation uses `source` and `dest`. This keeps common calls short and makes write intent visible before execution.

## `readMedia`

Each query requires `type` and `path`.

- `type:"image"`: returns a PNG, JPEG, GIF, or WebP directly to a vision-capable model.
- `type:"video"`: `view:"metadata"`, `"frame"`, or `"contactSheet"` (default).
- `type:"audio"`: `view:"metadata"`, `"waveform"` (default), or `"spectrogram"`.

Shared optional controls are `at`, `count`, `columns`, `width`, `height`, and `timeoutSec`; fields irrelevant to the chosen type/view are ignored or rejected by the operation. Visual results are included as model image content and rendered inline where supported. Metadata remains text. The tool never creates a user-requested artifact and is allowed in plan mode.

Generated previews are limited to 4 MB before base64 allocation. Pi then applies
its model-compatibility image normalization before provider history is written.

## `media`

Each query requires `type`:

- `image`: exactly one of `svg` or `html`; optional `dest` saves the PNG.
- `pdf`: exactly one of `html`, `markdown`, or `images`; `dest` is required.
- `gif`: `source` + `dest`, with optional timing, dimensions, fps, and loop count.
- `trim`: `source` + `dest`, start/end timing, and optional re-encoding.
- `audio`: `source` + `dest`, with format/bitrate controls.
- `convert`: `source` + `dest`, with codec, format, dimensions, fps, and quality controls.

All output paths are workspace guarded. Existing destinations require `overwrite:true`. The tool is blocked in plan mode because every public operation can create or replace an artifact.

## Safety and runtime

- ffmpeg and ffprobe receive validated argv arrays; no shell interpolation is used.
- Input and output paths pass the shared path guard.
- Timeouts and abort signals terminate child processes.
- Visual output obeys the inline image size cap.
- UI-only preview pixels are stored once as private image artifacts. Tool-result
  details contain paths and metadata, never a duplicate base64 payload.
- ffprobe JSON collection is capped at 2 MB and reduced to the bounded metadata
  summary used by the tool; raw probe objects are not retained in session details.
- Chrome is required for HTML rendering and PDF generation; ffmpeg/ffprobe are required for audio/video operations.
- Missing runtime dependencies return actionable install guidance.

## Agent-contract budget

The runtime registration layer owns concise descriptions for every direct tool and recursively compacts schema descriptions. Contract tests enforce:

- at most 360 characters per top-level tool description;
- at most 180 characters per schema description;
- at most 45,000 combined description-and-schema characters across the entire direct palette.

Detailed operational guidance remains in prompt guidelines and documentation, so it is available when relevant without inflating every tool declaration.

## Acceptance criteria

- Exactly `readMedia`, `media`, and `runFfmpeg` appear in the public media palette.
- The complete direct palette contains 16 support tools plus one guarded override.
- Read-only media operations pass the plan-mode gate; media production is blocked.
- Unit tests cover image perception, audio/video routing, authoring, transformation,
  registration, and contract budgets.
- Package build, typecheck, lint, and tests pass after the change.
