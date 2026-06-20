import { createCache } from "$lib/cache";
import type { components } from "$lib/api/schema";
import { getSettings } from "$lib/features/settings/api";

type UserSettings = components["schemas"]["UserSettings"];

const settingsCache = createCache(() => getSettings());

export const getSettingsCached = () => settingsCache.get();
export const invalidateSettingsCache = () => settingsCache.invalidate();
export const setSettingsCache = (s: UserSettings) => settingsCache.set(s);
