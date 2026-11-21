// Emulation flows: plan fragments that override device, media, CPU, location, time zone and locale.
import type { FlowDefinition } from './flow-types.mjs';
import { FRAGMENT } from './fragment.mjs';

const emulateDevice: FlowDefinition = {
  name: 'emulate-device',
  area: 'emulation',
  description:
    'Override viewport, pixel ratio, touch, user agent. The user agent is optional.',
  params: {
    width: {
      type: 'number',
      required: true,
      min: 1,
      description: 'Viewport width in CSS pixels',
    },
    height: {
      type: 'number',
      required: true,
      min: 1,
      description: 'Viewport height in CSS pixels',
    },
    deviceScaleFactor: {
      type: 'number',
      default: 1,
      min: 0,
      description: 'Device pixel ratio (0 keeps the browser default)',
    },
    mobile: {
      type: 'boolean',
      default: false,
      description:
        'Emulate a mobile device (meta viewport, overlay scrollbars)',
    },
    touch: {
      type: 'boolean',
      default: false,
      description: 'Emulate touch input (pointer: coarse)',
    },
    userAgent: {
      type: 'string',
      description: 'User-agent string to send and expose (optional)',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Emulation.setDeviceMetricsOverride',
        params: {
          width: '{{width}}',
          height: '{{height}}',
          deviceScaleFactor: '{{deviceScaleFactor}}',
          mobile: '{{mobile}}',
        },
      },
      {
        op: 'cdp',
        method: 'Emulation.setTouchEmulationEnabled',
        params: { enabled: '{{touch}}' },
      },
      {
        op: 'cdp',
        method: 'Network.setUserAgentOverride',
        params: { userAgent: '{{userAgent}}' },
      },
    ],
  },
  expects: [
    'The emulated viewport is width x height: with mobile false, window.innerWidth/innerHeight and visualViewport report it. With mobile true the mobile layout of Chrome can report a different innerWidth (about 980 px for a page without <meta name=viewport>), so read it to confirm, while screen.width/height report the emulated size. matchMedia("(pointer: coarse)") is true when touch is true',
    FRAGMENT,
  ],
};

const emulateMedia: FlowDefinition = {
  name: 'emulate-media',
  area: 'emulation',
  description: 'Override color scheme and reduced motion. Pass at least one.',
  params: {
    colorScheme: {
      type: 'string',
      enum: ['light', 'dark', 'no-preference'],
      description: 'prefers-color-scheme value (optional)',
    },
    reducedMotion: {
      type: 'string',
      enum: ['reduce', 'no-preference'],
      description: 'prefers-reduced-motion value (optional)',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Emulation.setEmulatedMedia',
        params: {
          features: [
            { name: 'prefers-color-scheme', value: '{{colorScheme}}' },
            { name: 'prefers-reduced-motion', value: '{{reducedMotion}}' },
          ],
        },
      },
    ],
  },
  expects: [
    'matchMedia("(prefers-color-scheme: dark)").matches is true for colorScheme dark; an absent feature is not overridden',
    FRAGMENT,
  ],
};

const cpuThrottle: FlowDefinition = {
  name: 'cpu-throttle',
  area: 'emulation',
  description: 'Slow the CPU by a factor (1 = none).',
  params: {
    rate: {
      type: 'number',
      required: true,
      min: 1,
      description: 'Slowdown factor, at least 1',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Emulation.setCPUThrottlingRate',
        params: { rate: '{{rate}}' },
      },
    ],
  },
  expects: [
    'Page scripts run rate times slower until the connection closes',
    FRAGMENT,
  ],
};

const emulateGeolocation: FlowDefinition = {
  name: 'emulate-geolocation',
  area: 'emulation',
  sensitive: true,
  description:
    'Override navigator.geolocation. Sensitive: needs acknowledge true and a granted geolocation permission (see grant-permission).',
  params: {
    latitude: {
      type: 'number',
      required: true,
      min: -90,
      max: 90,
      description: 'Latitude in degrees',
    },
    longitude: {
      type: 'number',
      required: true,
      min: -180,
      max: 180,
      description: 'Longitude in degrees',
    },
    accuracy: {
      type: 'number',
      default: 10,
      min: 0,
      description: 'Accuracy in meters',
    },
    acknowledge: {
      type: 'boolean',
      required: true,
      description: 'Must be true: confirms a location override is intended',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Emulation.setGeolocationOverride',
        acknowledge: '{{acknowledge}}',
        params: {
          latitude: '{{latitude}}',
          longitude: '{{longitude}}',
          accuracy: '{{accuracy}}',
        },
      },
    ],
  },
  expects: [
    'navigator.geolocation reports the given position once the origin has the geolocation permission',
    FRAGMENT,
  ],
};

const emulateTimezoneLocale: FlowDefinition = {
  name: 'emulate-timezone-locale',
  area: 'emulation',
  description:
    'Override the time zone and/or the locale. Pass at least one; a time zone needs acknowledge true.',
  params: {
    timezone: {
      type: 'string',
      description: 'IANA time zone id, e.g. Europe/Berlin (optional)',
    },
    locale: {
      type: 'string',
      description: 'BCP 47 locale, e.g. de-DE (optional)',
    },
    acknowledge: {
      type: 'boolean',
      description:
        'Must be true together with timezone: changing the time zone sites see needs the user permission',
    },
  },
  plan: {
    steps: [
      {
        op: 'cdp',
        method: 'Emulation.setTimezoneOverride',
        acknowledge: '{{acknowledge}}',
        params: { timezoneId: '{{timezone}}' },
      },
      {
        op: 'cdp',
        method: 'Emulation.setLocaleOverride',
        params: { locale: '{{locale}}' },
      },
    ],
  },
  expects: [
    'Intl.DateTimeFormat().resolvedOptions() reports the overridden time zone and locale',
    FRAGMENT,
  ],
};

export const emulationFlows: FlowDefinition[] = [
  emulateDevice,
  emulateMedia,
  cpuThrottle,
  emulateGeolocation,
  emulateTimezoneLocale,
];
