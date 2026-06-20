import { writable } from "svelte/store";

export const friendsSyncRevision = writable(0);

export function notifyFriendsSync(): void {
  friendsSyncRevision.update((n) => n + 1);
}
