import type { components } from "$lib/api/schema";
import { acceptFriend, listFriends, removeFriend } from "$lib/features/friends/api";
import { invalidateFriendsCache } from "$lib/features/friends/cache";
import { notifyFriendsSync } from "$lib/features/friends/sync";
import { invalidateRecordsCache } from "$lib/features/records/cache";
import { finalizeShare, listPendingShares } from "$lib/features/splits/api";

type FriendshipRelation = components["schemas"]["FriendshipRelation"];
type PendingShare = components["schemas"]["PendingShare"];

export type InboxItem =
  | { kind: "friend"; key: string; friend: FriendshipRelation }
  | { kind: "share"; key: string; share: PendingShare };

export async function loadPendingInbox(): Promise<InboxItem[]> {
  const [friendsResult, sharesResult] = await Promise.allSettled([
    listFriends({ pending: true, limit: 1000 }),
    listPendingShares({ limit: 1000 }),
  ]);

  const items: InboxItem[] = [];

  if (friendsResult.status === "fulfilled") {
    for (const friend of friendsResult.value.friends) {
      items.push({ kind: "friend", key: `friend:${friend.user_id}`, friend });
    }
  }

  if (sharesResult.status === "fulfilled") {
    for (const share of sharesResult.value) {
      items.push({ kind: "share", key: `share:${share.participant_id}`, share });
    }
  }

  return items;
}

export async function acceptPendingFriend(friendUserId: string): Promise<void> {
  await acceptFriend(friendUserId);
  invalidateFriendsCache();
  notifyFriendsSync();
}

export async function declinePendingFriend(friendUserId: string): Promise<void> {
  await removeFriend(friendUserId);
  invalidateFriendsCache();
  notifyFriendsSync();
}

export async function savePendingShare(participantId: string, categoryId: string): Promise<void> {
  await finalizeShare(participantId, categoryId);
  invalidateRecordsCache();
}

export function isAlreadyHandled(error: unknown): boolean {
  const status = (error as { status?: number }).status;
  return status === 404 || status === 409;
}
