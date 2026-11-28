/* eslint-disable @typescript-eslint/no-require-imports */
import { Platform } from "react-native";

import { noopProvider } from "./noop";
import type { HealthProvider } from "./types";

export type { HealthProvider } from "./types";

const cache = new Map<string, HealthProvider>();

/**
 * The health provider for this platform. Native modules are `require`d lazily inside the
 * platform files, so Expo Go, web and jest get the no-op provider instead of a crash.
 * HealthKit / Health Connect need a development build (see docs/decisions.md).
 */
/** True inside the Expo Go app, which cannot load HealthKit / Health Connect native modules. */
export function isExpoGo(): boolean {
  try {
    const Constants = require("expo-constants").default;
    return Constants?.executionEnvironment === "storeClient";
  } catch {
    return false;
  }
}

export function getHealthProvider(os: string = Platform.OS, expoGo: boolean = isExpoGo()): HealthProvider {
  const key = `${os}:${expoGo}`;
  const hit = cache.get(key);
  if (hit) return hit;
  let p: HealthProvider = noopProvider;
  // Never touch the native modules in Expo Go: even a guarded require logs a red error there.
  if (expoGo) p = noopProvider;
  else if (os === "ios") p = require("./ios").createIosProvider();
  else if (os === "android") p = require("./android").createAndroidProvider();
  cache.set(key, p);
  return p;
}
