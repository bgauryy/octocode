import { createRequire } from 'node:module'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const benchmarkRoot = dirname(fileURLToPath(import.meta.url))
export const packageRoot = join(benchmarkRoot, '..')
export const engineRoot = join(packageRoot, '..', 'octocode-engine')

// Canonical engine loader — every runner uses this one (CJS napi binary), so
// there is no index.js-vs-index.cjs drift across scripts.
export const requireEngine = createRequire(import.meta.url)
export const engine = requireEngine(join(engineRoot, 'index.cjs'))

/** Right-pad for aligned table columns (shared by every runner). */
export function pad(value, width) {
  return String(value).padEnd(width)
}

/**
 * Load `<areaDir>/manifest.json` and index its samples by extension. The
 * manifest records `sha256` + `bytes` per sample; `readSample` ENFORCES them so
 * a corrupted/edited/truncated fixture fails loudly instead of passing silently.
 */
export function loadManifestSamples(areaDir, sampleDir = join(benchmarkRoot, 'samples')) {
  const manifest = JSON.parse(readFileSync(join(areaDir, 'manifest.json'), 'utf8'))
  if (!Array.isArray(manifest.samples)) {
    throw new Error(`${join(areaDir, 'manifest.json')}: "samples" must be an array`)
  }
  const byExt = new Map(manifest.samples.map((s) => [s.ext, s]))

  // Resolve + integrity-check the sample for `ext`; pushes a precise issue (and
  // returns content '') on missing file, byte drift, or sha256 mismatch.
  function readSample(ext, issues) {
    const sample = byExt.get(ext)
    if (!sample) {
      issues.push(`no sample in manifest.json for .${ext}`)
      return { sample: null, content: '' }
    }
    const path = join(sampleDir, sample.file)
    if (!existsSync(path)) {
      issues.push(`sample file missing: ${sample.file}`)
      return { sample, content: '' }
    }
    const buf = readFileSync(path)
    if (typeof sample.bytes === 'number' && buf.length !== sample.bytes) {
      issues.push(`${sample.file}: bytes ${buf.length} != manifest ${sample.bytes}`)
    }
    if (sample.sha256) {
      const sha = createHash('sha256').update(buf).digest('hex')
      if (sha !== sample.sha256) {
        issues.push(`${sample.file}: sha256 mismatch (fixture changed — re-pin or restore)`)
        return { sample, content: '' }
      }
    }
    return { sample, content: buf.toString('utf8'), path }
  }

  return { manifest, sampleDir, byExt, readSample }
}
