import type { HealthProvider } from './types';

/** Used on web, in Expo Go, in tests and anywhere the native modules are missing. */
export const noopProvider: HealthProvider = {
  available: false,
  requestPermissions: async () => false,
  readSteps: async () => [],
  readWorkouts: async () => [],
  readSleep: async () => [],
};
