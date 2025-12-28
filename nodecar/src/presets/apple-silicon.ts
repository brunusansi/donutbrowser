/**
 * Apple Silicon Hardware Presets
 * Provides strict hardware configurations for consistent fingerprinting
 */

export interface HardwarePreset {
  name: string;
  renderer: string;
  vendor: string;
  cores: number;
  memory: number; // in GB
  screen: {
    width: number;
    height: number;
  };
}

/**
 * Apple Silicon Preset Registry
 * Based on official Apple specifications
 */
export const APPLE_SILICON_PRESETS: Record<string, HardwarePreset> = {
  // M1 Series
  "M1": {
    name: "Apple M1",
    renderer: "Apple M1",
    vendor: "Apple",
    cores: 8,
    memory: 8,
    screen: {
      width: 2560,
      height: 1600,
    },
  },
  "M1-Pro": {
    name: "Apple M1 Pro",
    renderer: "Apple M1 Pro",
    vendor: "Apple",
    cores: 10,
    memory: 16,
    screen: {
      width: 3024,
      height: 1964,
    },
  },
  "M1-Max": {
    name: "Apple M1 Max",
    renderer: "Apple M1 Max",
    vendor: "Apple",
    cores: 10,
    memory: 32,
    screen: {
      width: 3456,
      height: 2234,
    },
  },
  "M1-Ultra": {
    name: "Apple M1 Ultra",
    renderer: "Apple M1 Ultra",
    vendor: "Apple",
    cores: 20,
    memory: 64,
    screen: {
      width: 5120,
      height: 2880,
    },
  },

  // M2 Series
  "M2": {
    name: "Apple M2",
    renderer: "Apple M2",
    vendor: "Apple",
    cores: 8,
    memory: 8,
    screen: {
      width: 2560,
      height: 1600,
    },
  },
  "M2-Pro": {
    name: "Apple M2 Pro",
    renderer: "Apple M2 Pro",
    vendor: "Apple",
    cores: 12,
    memory: 16,
    screen: {
      width: 3024,
      height: 1964,
    },
  },
  "M2-Max": {
    name: "Apple M2 Max",
    renderer: "Apple M2 Max",
    vendor: "Apple",
    cores: 12,
    memory: 32,
    screen: {
      width: 3456,
      height: 2234,
    },
  },
  "M2-Ultra": {
    name: "Apple M2 Ultra",
    renderer: "Apple M2 Ultra",
    vendor: "Apple",
    cores: 24,
    memory: 64,
    screen: {
      width: 5120,
      height: 2880,
    },
  },

  // M3 Series
  "M3": {
    name: "Apple M3",
    renderer: "Apple M3",
    vendor: "Apple",
    cores: 8,
    memory: 8,
    screen: {
      width: 2560,
      height: 1600,
    },
  },
  "M3-Pro": {
    name: "Apple M3 Pro",
    renderer: "Apple M3 Pro",
    vendor: "Apple",
    cores: 12,
    memory: 18,
    screen: {
      width: 3024,
      height: 1964,
    },
  },
  "M3-Max": {
    name: "Apple M3 Max",
    renderer: "Apple M3 Max",
    vendor: "Apple",
    cores: 16,
    memory: 36,
    screen: {
      width: 3456,
      height: 2234,
    },
  },

  // M4 Series
  "M4": {
    name: "Apple M4",
    renderer: "Apple M4",
    vendor: "Apple",
    cores: 10,
    memory: 16,
    screen: {
      width: 2560,
      height: 1600,
    },
  },
  "M4-Pro": {
    name: "Apple M4 Pro",
    renderer: "Apple M4 Pro",
    vendor: "Apple",
    cores: 14,
    memory: 24,
    screen: {
      width: 3024,
      height: 1964,
    },
  },
  "M4-Max": {
    name: "Apple M4 Max",
    renderer: "Apple M4 Max",
    vendor: "Apple",
    cores: 16,
    memory: 36,
    screen: {
      width: 3456,
      height: 2234,
    },
  },
};

/**
 * Get a hardware preset by key
 */
export function getHardwarePreset(presetKey: string): HardwarePreset | undefined {
  return APPLE_SILICON_PRESETS[presetKey];
}

/**
 * Get all available preset keys
 */
export function getAvailablePresets(): string[] {
  return Object.keys(APPLE_SILICON_PRESETS);
}

/**
 * Get preset display name
 */
export function getPresetDisplayName(presetKey: string): string {
  const preset = APPLE_SILICON_PRESETS[presetKey];
  return preset ? preset.name : presetKey;
}
