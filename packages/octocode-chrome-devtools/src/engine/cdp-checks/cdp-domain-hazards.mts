// The one list of methods the cdp-domain dump never sends, however a name matches: each can crash,
// hang, strand the page or change browser state. docs/cdp-protocol.md renders this list and a test
// holds the two together. A method a Chrome does not have is skipped as absent, never an error.

export type Hazard = {
  /** `Domain.method` */
  method: string;
  /** One line: what goes wrong when it is sent blindly. */
  reason: string;
};

const hazard = (method: string, reason: string): Hazard => ({
  method,
  reason,
});

export const CDP_DOMAIN_HAZARDS: readonly Hazard[] = [
  hazard('Emulation.setDisplayFeaturesOverride', 'crashes the browser'),
  hazard('Storage.deleteStorageBucket', 'crashes the browser'),
  hazard('Input.synthesizePinchGesture', 'hangs without answering'),
  hazard('Page.waitForDebugger', 'hangs without answering'),
  hazard('Storage.runBounceTrackingMitigations', 'hangs without answering'),
  hazard(
    'Memory.getDOMCountersForLeakDetection',
    'leaves the page unresponsive'
  ),
  hazard('Memory.prepareForLeakDetection', 'leaves the page unresponsive'),
  hazard('Memory.forciblyPurgeJavaScriptMemory', 'discards page memory'),
  hazard('Debugger.pause', 'pauses page execution'),
  hazard('Debugger.resume', 'changes page execution'),
  hazard('Debugger.stepInto', 'changes page execution'),
  hazard('Debugger.stepOver', 'changes page execution'),
  hazard('Debugger.stepOut', 'changes page execution'),
  hazard('Runtime.terminateExecution', 'stops running page scripts'),
  hazard(
    'Runtime.runIfWaitingForDebugger',
    'releases a target waiting for its debugger'
  ),
  hazard('Page.stopLoading', 'cancels the page load'),
  hazard('Page.reload', 'reloads the page'),
  hazard('Page.startScreencast', 'starts a frame stream'),
  hazard('Page.startScreenRecording', 'starts a recording'),
  hazard('Profiler.start', 'starts a profile that stays on'),
  hazard('Profiler.startPreciseCoverage', 'starts coverage that stays on'),
  hazard('HeapProfiler.startSampling', 'starts a profile that stays on'),
  hazard(
    'HeapProfiler.startTrackingHeapObjects',
    'starts tracking that stays on'
  ),
  hazard('Tracing.start', 'starts a trace that stays on'),
  hazard('Tracing.end', 'ends a trace another client started'),
  hazard('Browser.resetPermissions', 'resets every site permission'),
  hazard('Log.clear', 'erases log entries'),
  hazard('Network.clearBrowserCache', 'erases the browser cache'),
  hazard('ServiceWorker.stopAllWorkers', 'stops every service worker'),
  hazard('Target.createBrowserContext', 'creates a browser context'),
  hazard('Target.attachToBrowserTarget', 'attaches a new session'),
];

const byMethod = new Map(
  CDP_DOMAIN_HAZARDS.map(entry => [entry.method, entry])
);

/** The hazard entry of `Domain.method`, if it is on the list. */
export const hazardOf = (method: string): Hazard | undefined =>
  byMethod.get(method);
