import type { PlatformId } from './platform.js';

export type ArchiveKind = 'none' | 'gz' | 'zip' | 'tar.gz' | 'tar.xz';

export interface ManifestAsset {
  url: string;
  archive: ArchiveKind;
  binName: string;
  /** Path of the executable inside the archive (zip/tar); absent for gz/none. */
  binPath?: string;
  /** SHA-256 of the downloaded asset; download is refused while this is null. */
  sha256: string | null;
}

export interface ManifestServer {
  languageId: string;
  repo: string;
  releaseTag: string;
  launchArgs?: string[];
  downloadHost?: string;
  platforms: Partial<Record<PlatformId, ManifestAsset>>;
  unsupportedPlatforms?: Partial<Record<PlatformId, string>>;
}

export interface ManifestFile {
  $comment?: string;
  version: number;
  servers: Record<string, ManifestServer>;
}
