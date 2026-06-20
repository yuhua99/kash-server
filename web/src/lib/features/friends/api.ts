import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type PublicUser = components["schemas"]["PublicUser"];
type FriendshipRelation = components["schemas"]["FriendshipRelation"];
type FriendListResponse = components["schemas"]["FriendListResponse"];
type RemoveFriendResponse = components["schemas"]["RemoveFriendResponse"];

export function searchUsers(params: {
  query: string;
  limit?: number;
  offset?: number;
}): Promise<PublicUser[]> {
  return client.get("/friends/search", {
    query: params.query,
    limit: params.limit,
    offset: params.offset,
  });
}

export function listFriends(params: {
  pending?: boolean;
  limit?: number;
  offset?: number;
}): Promise<FriendListResponse> {
  return client.get("/friends/list", {
    pending: params.pending,
    limit: params.limit,
    offset: params.offset,
  });
}

export function sendFriendRequest(friendUsername: string): Promise<FriendshipRelation> {
  return client.post("/friends/request", { friend_username: friendUsername });
}

export function acceptFriend(friendUserId: string): Promise<FriendshipRelation> {
  return client.post("/friends/accept", { friend_id: friendUserId });
}

export function removeFriend(friendUserId: string): Promise<RemoveFriendResponse> {
  return client.post("/friends/remove", { friend_id: friendUserId });
}

export function updateNickname(
  friendUserId: string,
  nickname: string | null,
): Promise<FriendshipRelation> {
  return client.patch("/friends/nickname", { friend_id: friendUserId, nickname });
}
