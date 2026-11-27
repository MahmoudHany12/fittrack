/* eslint-disable @typescript-eslint/no-require-imports */
import { Platform } from 'react-native';

import { noopProvider } from './noop';
import type { HealthProvider } from './types';

export type { HealthProvider } from './types';

const cache = new Map<string, HealthProvider>();

/**
 * The health provider for this platform. Native modules are `require`d lazily inside the
 * platform files, so Expo Go, web and jest get the no-op provider instead of a crash.
 * HealthKit / Health Connect need a development build (see docs/decisions.md).
 */
export function getHealthProvider(os: string = Platform.OS): HealthProvider {
  const hit = cache.get(os);
  if (hit) return hit;
  let p: HealthProvider = noopProvider;
  if (os === 'ios') p = require('./ios').createIosProvider();
  else if (os === 'android') p = require('./android').createAndroidProvider();
  cache.set(os, p);
  return p;
}
