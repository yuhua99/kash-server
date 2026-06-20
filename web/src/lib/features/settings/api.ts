import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type UserSettings = components["schemas"]["UserSettings"];

export function getSettings(): Promise<UserSettings> {
  return client.get<UserSettings>("/settings");
}

export function updateSettings(mainCurrency: string): Promise<UserSettings> {
  return client.put<UserSettings>("/settings", { main_currency: mainCurrency });
}
