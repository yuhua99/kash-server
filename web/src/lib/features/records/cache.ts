import { createCache } from "$lib/cache";
import { getRecords } from "$lib/features/records/api";

const recordsCache = createCache(() =>
  getRecords({ limit: 500, offset: 0 }).then((r) => r.records),
);

export const getRecentRecordsCached = () => recordsCache.get();
export const invalidateRecordsCache = () => recordsCache.invalidate();
