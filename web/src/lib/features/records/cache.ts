import type { components } from "$lib/api/schema";
import { createCache } from "$lib/cache";
import { getRecords } from "$lib/features/records/api";

type RecordItem = components["schemas"]["Record"];

const recordsCache = createCache(() =>
  getRecords({ limit: 500, offset: 0 }).then((r) => r.records),
);

export const getRecentRecordsCached = () => recordsCache.get();
export const invalidateRecordsCache = () => recordsCache.invalidate();

export function filterRecordsByDateRange(
  records: RecordItem[],
  start: string,
  end: string,
): RecordItem[] {
  return records.filter((record) => {
    if (start && record.date < start) {
      return false;
    }
    if (end && record.date > end) {
      return false;
    }
    return true;
  });
}
