import { createCache } from "$lib/cache";
import { listFriends } from "$lib/features/friends/api";

const friendsCache = createCache(() =>
  listFriends({ pending: false, limit: 1000, offset: 0 }).then((response) => response.friends),
);

export const getAcceptedFriendsCached = () => friendsCache.get();
export const invalidateFriendsCache = () => friendsCache.invalidate();
