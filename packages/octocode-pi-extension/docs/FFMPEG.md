# FFmpeg — Reference & Cookbook

Single source of truth for every ffmpeg-related capability in the Pi extension.
Grounded in `media-tool.ts`, `read-media-tool.ts`, and `run-ffmpeg-tool.ts`.

---

## Contents

1. [Discovery & runtime](#1-discovery--runtime)
2. [Exact argvs](#2-exact-argvs)
3. [Tool routing](#3-tool-routing)
4. [`runFfmpeg` reference](#4-runffmpeg-reference)
5. [Anti-patterns & gotchas](#5-anti-patterns--gotchas)
6. [Cookbook — 16 recipes](#6-cookbook)
7. [Capability matrix](#7-capability-matrix)

---

## 1. Discovery & Runtime

`ffmpeg-runtime.ts` resolves `ffmpeg` + `ffprobe` once on first use, then caches.

**Search order:** `$PATH` → `/opt/homebrew/bin` → `/usr/local/bin` → `/usr/bin` → `ffmpeg-static` (optional dep fallback)

**Security model:** every call uses `spawn(bin, argv[])` — no shell, no injection possible.

| Limit | Value |
|---|---|
| Wall-clock timeout | 120 s (override with `timeoutSec`) |
| Stdout cap (piped) | 32 MB |
| Termination | SIGTERM → SIGKILL |

When ffmpeg is missing, tools throw with install instructions:
```
brew install ffmpeg      # macOS
apt install ffmpeg       # Debian/Ubuntu
https://ffmpeg.org/download.html
```

---

## 2. Exact Argvs

### `readMedia` — inspection only

| View | Command |
|---|---|
| `metadata` | `ffprobe -v quiet -print_format json -show_streams -show_format <input>` |
| `frame` | `ffmpeg -y -ss <at> -i <in> [-vf scale=W:-1] -frames:v 1 <tmp.png>` |
| `contactSheet` | `ffmpeg -y -i <in> -vf fps=<n/dur>,scale=<w>:-1,tile=<c>x<r> -frames:v 1 <tmp.png>` |
| `waveform` | `ffmpeg -y -i <in> -lavfi showwavespic=s=<W>x<H>:colors=#3b82f6 -frames:v 1 <tmp.png>` |
| `spectrogram` | `ffmpeg -y -i <in> -lavfi showspectrumpic=s=<W>x<H>:legend=disabled -frames:v 1 <tmp.png>` |

Defaults: `frame` at `"0"` · `contactSheet` count=9, columns=ceil(√count), width=320 · `waveform` 640×160 · `spectrogram` 640×320.

### `media` — writes output files

**`gif`**
```
ffmpeg -y [-ss from] [-to to] -i <in>
  -filter_complex fps=<fps>,scale=<w>:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse
  <out.gif>
```
Defaults: `fps:12`, `width:480`. Palette pipeline always on.

**`trim`**
```
# Stream copy (default) — fast, ±1–2 s keyframe boundary:
ffmpeg -y -ss <from> -i <in> [-to <to>|-t <dur>] -c copy <out>

# Re-encode (reencode:true) — frame-accurate:
ffmpeg -y -ss <from> -i <in> [-to <to>|-t <dur>]
  -c:v <videoCodec|libx264> [-pix_fmt yuv420p] -c:a <audioCodec|aac> <out>
```
`videoCodec`/`audioCodec` are respected when `reencode:true`.

**`audio`**
```
ffmpeg -y -i <in> -vn -c:a <codec> [-b:a <N>k] <out>
```
Codec map: `mp3→libmp3lame`, `aac→aac`, `wav→pcm_s16le`, `flac→flac`.

**`convert`**
```
ffmpeg -y -i <in> [-vf scale=<W>:<H>[,fps=<fps>]]
  -c:v <vcodec> [-pix_fmt yuv420p] [-crf <N>] [-b:v <rate>]
  [-c:a <acodec> | -an] <out>
```
Video codecs: `h264→libx264` (default), `hevc→libx265`, `vp9→libvpx-vp9`, `av1→libsvtav1`, `h264_videotoolbox`, `hevc_videotoolbox`, `copy`.
Audio codecs: `aac` (default), `mp3→libmp3lame`, `copy`, `none→-an`.
HW codecs (`*_videotoolbox`) skip `-pix_fmt`/`-crf`, use `-b:v` instead.
Default CRF: 23.

**`concat`**
```
ffmpeg -y -f concat -safe 0 -i <list.txt> [-c copy | -c:v <vc> -c:a <ac>] <out>
```
List file is written to `.octocode/media-tmp/` and cleaned up after the call.

---

## 3. Tool Routing

```
Inspect content?                         →  readMedia
Produce a file (common transforms)?      →  media
HW encode / overlay / normalize / VMAF?  →  runFfmpeg
Anything else?                           →  runFfmpeg
```

### Cost ladder — cheapest first

```
readMedia:
  image file (png/jpg/gif/webp, < 4 MB)  →  type:"image"          (no ffmpeg)
  video: codec / duration / fps          →  view:"metadata"       (ffprobe only)
  video: spot a moment                   →  view:"frame"
  video: whole-clip overview             →  view:"contactSheet"
  audio: amplitude / silence             →  view:"waveform"
  audio: frequencies / noise             →  view:"spectrogram"

media:
  animated preview                       →  type:"gif"
  cut a segment                          →  type:"trim"
  strip video, keep audio                →  type:"audio"
  reformat / resize / recompress         →  type:"convert"  (videoCodec:"h264_videotoolbox" for HW)
  join multiple files                    →  type:"concat"
  render HTML / Markdown / SVG           →  type:"image" or type:"pdf"

runFfmpeg (path-guarded, no shell):
  side-by-side / stacked layout          →  hstack / vstack
  text watermark                         →  -vf drawtext (needs libfreetype)
  audio normalize (EBU R128)             →  -af loudnorm
  silence detection                      →  -af silencedetect
  speed change                           →  -vf setpts + -af atempo
  screen recording                       →  -f avfoundation  (needs Screen Recording perm)
  VMAF quality score                     →  -lavfi libvmaf
  ProRes master                          →  -c:v prores_videotoolbox
  any other ffmpeg operation             →  runFfmpeg(args:[...])
```

### Hardware acceleration (macOS — VideoToolbox)

| Encoder | `media convert` | `runFfmpeg` | Speed-up |
|---|---|---|---|
| H.264 | `videoCodec:"h264_videotoolbox"` ✅ | `h264_videotoolbox` ✅ | ~8× |
| HEVC | `videoCodec:"hevc_videotoolbox"` ✅ | `hevc_videotoolbox` ✅ | ~6× |
| ProRes 422 | — | `prores_videotoolbox` ✅ | ~10× |

Rule: `media convert` + VideoToolbox for file delivery; `runFfmpeg` for ProRes/ProRes HQ masters.

### CRF guide (`h264`/`hevc`/`vp9`)

| CRF | Quality |
|---|---|
| 18 | Near-lossless / archival |
| 23 | Good (default) |
| 28 | Smaller / mobile |
| 32 | Draft |

### ContactSheet sizing

| Length | `count` | `columns` |
|---|---|---|
| < 60 s | 9 | 3 |
| 1–10 min | 16 | 4 |
| 10–60 min | 25 | 5 |
| > 1 h | 36 | 6 |

---

## 4. `runFfmpeg` Reference

Thin, path-guarded wrapper. Same security model as other tools: `spawn(bin, argv[])`, no shell.

**Source:** `src/tools/run-ffmpeg-tool.ts`

```typescript
runFfmpeg({
  args:           string[],            // ffmpeg argv WITHOUT binary name (required)
  binary?:        'ffmpeg'|'ffprobe',  // default 'ffmpeg'
  captureStdout?: boolean,             // save stdout bytes privately and return the artifact path
  timeoutSec?:    number,              // default 120, max 1800
})
```

**Path resolution:** args containing `/`, `./`, `../`, or a file extension — and args after `-i` — are resolved relative to `cwd` and validated before the process starts.

**Progress:** `time=` and `speed=` forwarded as streaming updates.

**Output:** returns the last meaningful ffmpeg summary line on success; throws
with exit code plus a bounded stderr tail on failure. With `captureStdout:true`,
binary stdout is always written losslessly to the private session artifact tree—even
when Pi did not supply an explicit session context—and only its byte count and path
enter the tool result. ffprobe JSON collection is capped at 2 MB.

---

## 5. Anti-patterns & Gotchas

**`readMedia image` on files > 4 MB** — scale first:
```bash
ffmpeg -y -i /tmp/huge.png -vf scale=1920:-1 /tmp/sm.png
readMedia type:"image" path:"/tmp/sm.png"
```

**`reencode:true` for rough cuts** — stream copy is instant and sufficient for most edits.

**`contactSheet` count >> clip duration** — rule: ~1 frame per 2–3 s of content.

**GIF from a long source** — trim to 5–10 s first, then GIF.

**Seeking past end of clip** — check metadata first (`readMedia view:"metadata"`) to confirm `durationSec`.

**`drawtext` filter missing** — requires `--enable-libfreetype`. Fix: `brew install freetype && brew reinstall ffmpeg`. Check: `ffmpeg -filters | grep drawtext`.

**`-ss before -i` fast-seek** — lands on the nearest keyframe (±1–2 s). For frame-exact work: `trim reencode:true` to the range, then `readMedia frame at:"0"` on the result.

**`yuv420p` for intermediates** — `convert` always adds `-pix_fmt yuv420p` (correct for delivery, lossy for chroma). For intermediates, use `runFfmpeg` directly.

---

## 6. Cookbook

### Recipe 1: Screenshot → model sees screen

*Requires Screen Recording — System Settings → Privacy & Security.*

```bash
screencapture -x /tmp/screen.png          # full screen, silent
screencapture -x -m /tmp/screen.png       # main monitor only
```
```
readMedia(type:"image", path:"/tmp/screen.png")
```
If > 4 MB: `ffmpeg -y -i /tmp/screen.png -vf scale=1920:-1 /tmp/sm.png` first.

---

### Recipe 2: Understand a video in two calls

```
# 1 — metadata (instant, ffprobe only)
readMedia(type:"video", view:"metadata", path:"input.mp4")

# 2 — contact sheet (whole arc)
readMedia(type:"video", view:"contactSheet", path:"input.mp4", count:16, columns:4)

# 3 (optional) — zoom into a moment
readMedia(type:"video", view:"frame", path:"input.mp4", at:"1:03")
```

---

### Recipe 3: Extract a highlight clip

```
# Fast — stream copy, ±1–2 s keyframe boundary
media(type:"trim", source:"input.mp4", from:"1:23", to:"1:35", dest:"clip.mp4")

# Exact — frame-accurate, ~10× slower
media(type:"trim", source:"input.mp4", from:"1:23", to:"1:35", reencode:true, dest:"clip.mp4")

# Different output codec — trim first (stream copy), then convert:
media(type:"convert", source:"clip.mp4", videoCodec:"hevc", crf:28, dest:"clip_hevc.mp4")
```

---

### Recipe 4: Make a shareable GIF

```
media(type:"gif", source:"input.mp4",
  from:"0:44", to:"0:49", fps:12, width:480,
  dest:"highlight.gif")
```
**Always set `from`/`to`.** A 2-minute source at 12 fps = 1440-frame GIF.

---

### Recipe 5: Audio quality check

```
readMedia(type:"audio", view:"metadata",    path:"track.mp3")   # codec, bitrate, duration
readMedia(type:"audio", view:"waveform",    path:"track.mp3", width:900, height:200)
readMedia(type:"audio", view:"spectrogram", path:"track.mp3", width:900, height:400)
```

---

### Recipe 6: Normalize audio for delivery

```
# One-pass (EBU R128, good enough for most work)
runFfmpeg(args:["-y", "-i", "input.mp3",
  "-af", "loudnorm=I=-16:TP=-1.5:LRA=11",
  "normalized.mp3"])
```

---

### Recipe 7: Convert & compress for web

```
# H.264 — widest compat
media(type:"convert", source:"raw.mp4",
  videoCodec:"h264", crf:23, scale:"1280x-1", audioCodec:"aac", dest:"web.mp4")

# HEVC — ~40% smaller
media(type:"convert", source:"raw.mp4", videoCodec:"hevc", crf:28, dest:"web_hevc.mp4")

# Audio-only
media(type:"audio", source:"interview.mp4", format:"mp3", bitrate:"192k", dest:"podcast.mp3")

# Strip audio
media(type:"convert", source:"input.mp4", videoCodec:"copy", audioCodec:"none", dest:"silent.mp4")
```

---

### Recipe 8: Fast Mac encode — hardware

```
# H.264 VideoToolbox (~8× faster)
media(type:"convert", source:"input.mp4",
  videoCodec:"h264_videotoolbox", bitrate:"4M", audioCodec:"copy", dest:"fast.mp4")

# HEVC VideoToolbox (~6× faster)
media(type:"convert", source:"input.mp4",
  videoCodec:"hevc_videotoolbox", bitrate:"3M", dest:"hevc.mp4")

# ProRes 422 HQ master (runFfmpeg — prores not in media)
runFfmpeg(args:["-y", "-i", "input.mp4",
  "-c:v", "prores_videotoolbox", "-profile:v", "3",
  "-c:a", "pcm_s16le", "master.mov"])
```

---

### Recipe 9: Concat clips

```
# Same codec — built-in concat type
media(type:"concat", sources:["clip1.mp4", "clip2.mp4", "clip3.mp4"], dest:"merged.mp4")

# Different codecs — re-encode
media(type:"concat", sources:["a.mp4", "b.mp4"], reencode:true,
  videoCodec:"h264", dest:"merged.mp4")

# Full filter_complex (different resolutions)
runFfmpeg(args:["-y", "-i", "a.mp4", "-i", "b.mp4",
  "-filter_complex", "[0:v][0:a][1:v][1:a]concat=n=2:v=1:a=1[v][a]",
  "-map", "[v]", "-map", "[a]",
  "-c:v", "libx264", "-crf", "23", "-c:a", "aac", "merged.mp4"])
```

---

### Recipe 10: Side-by-side comparison

```
runFfmpeg(args:["-y", "-i", "original.mp4", "-i", "compressed.mp4",
  "-filter_complex", "hstack",
  "-c:v", "libx264", "-crf", "23", "comparison.mp4"])
```

---

### Recipe 11: Burn a text watermark

> Requires `--enable-libfreetype`. Check: `ffmpeg -filters | grep drawtext`.

```
runFfmpeg(args:["-y", "-i", "input.mp4",
  "-vf", "drawtext=text='© 2025':fontsize=28:fontcolor=white@0.85:x=w-tw-20:y=h-th-20:shadowcolor=black@0.5:shadowx=2:shadowy=2",
  "-c:a", "copy", "watermarked.mp4"])
```

---

### Recipe 12: Find silence in audio

```
runFfmpeg(args:["-i", "podcast.mp3",
  "-af", "silencedetect=noise=-30dB:d=0.5",
  "-f", "null", "-"])
# Silence timestamps appear in the stderr summary
```

Visual: `readMedia(type:"audio", view:"waveform")` — silence shows as flat baseline.

---

### Recipe 13: Speed up / slow down

```
# 2× faster
runFfmpeg(args:["-y", "-i", "input.mp4",
  "-vf", "setpts=0.5*PTS", "-af", "atempo=2.0", "2x.mp4"])

# 4× (atempo max 2.0 — chain two filters)
runFfmpeg(args:["-y", "-i", "input.mp4",
  "-vf", "setpts=0.25*PTS", "-af", "atempo=2.0,atempo=2.0", "4x.mp4"])
```

---

### Recipe 14: Measure encode quality — VMAF

```
runFfmpeg(args:["-y",
  "-i", "compressed.mp4",   # distorted
  "-i", "original.mp4",    # reference
  "-lavfi", "[0:v][1:v]libvmaf=log_fmt=json:log_path=/tmp/vmaf.json",
  "-f", "null", "-"])
```

| VMAF | Quality |
|---|---|
| 95–100 | Transparent — lower bitrate |
| 85–94 | Excellent — good for delivery |
| 70–84 | Acceptable web |
| < 70 | Re-encode |

---

### Recipe 15: Record the screen

*Requires Screen Recording permission.*

```
runFfmpeg(args:["-f", "avfoundation", "-i", "1",
  "-t", "10", "-c:v", "h264_videotoolbox", "-b:v", "5M",
  "/tmp/screenrec.mp4"])
```

List devices: `ffmpeg -f avfoundation -list_devices true -i '' 2>&1`

---

### Recipe 16: Generate a test card

```
runFfmpeg(args:["-f", "lavfi", "-i", "smptebars=duration=10:size=1280x720:rate=30",
  "-f", "lavfi", "-i", "sine=frequency=1000:duration=10",
  "-c:v", "libx264", "-crf", "23", "-c:a", "aac", "/tmp/testcard.mp4"])
```

---

## 7. Capability Matrix

| Capability | `readMedia` | `media` | `runFfmpeg` |
|---|---|---|---|
| Read image pixels | ✅ (< 4 MB) | — | — |
| Video metadata | ✅ | — | ✅ (ffprobe) |
| Single frame | ✅ | — | — |
| Contact sheet | ✅ | — | — |
| Audio waveform | ✅ | — | — |
| Audio spectrogram | ✅ | — | — |
| Render HTML/SVG/MD | — | ✅ | — |
| GIF (palette quality) | — | ✅ | — |
| Trim clip | — | ✅ | — |
| Extract audio | — | ✅ | — |
| Convert / transcode | — | ✅ | — |
| HW encode (VideoToolbox) | — | ✅ | ✅ |
| Concat sources | — | ✅ | ✅ |
| Side-by-side / overlay | — | — | ✅ |
| Text watermark | — | — | ✅ ¹ |
| Normalize audio | — | — | ✅ |
| Silence detection | — | — | ✅ |
| Speed change | — | — | ✅ |
| VMAF quality score | — | — | ✅ |
| Screen recording | — | — | ✅ ² |
| ProRes master | — | — | ✅ |

¹ Requires `--enable-libfreetype` in ffmpeg build.  
² Requires Screen Recording permission (macOS).
