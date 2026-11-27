// Types only: the fields the opt-in stealth overrides keep on a CDP session.
import type { Payload, Session } from './cdp-connection.mjs';

/** The OS a user agent string names, in the shape its client hints report. */
export type UserAgentOs = {
  platform: string; // Sec-CH-UA-Platform
  mobile: boolean;
  platformVersion: string;
  architecture: string;
  bitness: string;
  model: string;
};

export type BrandVersion = { brand: string; version: string };

/** What the host probe reads from chrome://version (navigator values and high-entropy hints). */
export type HostProbe = {
  userAgent: string;
  platform: string;
  metadata: {
    brands?: BrandVersion[];
    fullVersionList?: BrandVersion[];
    platform?: string;
    [hint: string]: unknown;
  };
};

/** The identity the overrides present: the host browser without headless markers. */
export type HostIdentity = {
  userAgent: string;
  platform: string;
  userAgentMetadata: Payload | undefined;
};

export type AutoAttachParams = {
  autoAttach: boolean;
  waitForDebuggerOnStart: boolean;
  flatten: boolean;
};

export type TargetFailure = { targetId?: string; error: string };

export type StealthVerifyResult = {
  passed: number;
  failed: number;
  total: number;
  failures: string[];
};

/** A CDP session the stealth overrides decorate, so later steps reuse them. */
export type StealthSession = Session & {
  stealthApplied?: boolean;
  stealthVerify?: StealthVerifyResult;
  stealthIdentity?: HostIdentity;
  stealthChildFailures?: TargetFailure[];
  stealthAutoAttach?: AutoAttachParams;
};

/** `Target.attachedToTarget` event params. */
export type AttachedToTarget = {
  sessionId: string;
  targetInfo: { targetId: string; type: string; url: string };
  waitingForDebugger: boolean;
};
