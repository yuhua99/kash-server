import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type CreateSplitPayload = components["schemas"]["CreateSplitPayload"];
type SplitCreatedResponse = components["schemas"]["SplitCreatedResponse"];
type RecordItem = components["schemas"]["Record"];
type ShareStatusResponse = components["schemas"]["ShareStatusResponse"];
type PendingShare = components["schemas"]["PendingShare"];
type UnsettledShare = components["schemas"]["UnsettledShare"];
type PendingShareListResponse = components["schemas"]["PendingShareListResponse"];
type UnsettledShareListResponse = components["schemas"]["UnsettledShareListResponse"];
type SettleAllResponse = components["schemas"]["SettleAllResponse"];

export function createSplit(payload: CreateSplitPayload): Promise<SplitCreatedResponse> {
  return client.post<SplitCreatedResponse>("/splits", payload);
}

export function finalizeShare(participantId: string, categoryId: string): Promise<RecordItem> {
  return client.post<RecordItem>(`/splits/participants/${participantId}/finalize`, {
    category_id: categoryId,
  });
}

export function settleShare(participantId: string): Promise<ShareStatusResponse> {
  return client.put<ShareStatusResponse>(`/splits/participants/${participantId}/settle`);
}

export function listPendingShares(
  params: { limit?: number; offset?: number } = {},
): Promise<PendingShare[]> {
  return client
    .get<PendingShareListResponse>("/splits/pending", {
      limit: params.limit,
      offset: params.offset,
    })
    .then((r) => r.shares);
}

export function listUnsettledShares(
  friendId: string,
  params: { limit?: number; offset?: number } = {},
): Promise<UnsettledShare[]> {
  return client
    .get<UnsettledShareListResponse>("/splits/unsettled", {
      friend_id: friendId,
      limit: params.limit,
      offset: params.offset,
    })
    .then((r) => r.shares);
}

export function settleAllWithFriend(friendId: string): Promise<SettleAllResponse> {
  return client.put<SettleAllResponse>(`/splits/with/${friendId}/settle-all`);
}
