import type { components } from "$lib/api/schema";
import { getRecords } from "$lib/features/records/api";

type RecordItem = components["schemas"]["Record"];

const PAGE_SIZE = 1000;

export async function getAllRecordsByDateRange(params: {
  startDate?: string;
  endDate?: string;
}): Promise<RecordItem[]> {
  const all: RecordItem[] = [];
  let offset = 0;

  for (;;) {
    const page = await getRecords({
      start_date: params.startDate,
      end_date: params.endDate,
      limit: PAGE_SIZE,
      offset,
    });

    all.push(...page.records);

    if (page.records.length === 0 || all.length >= page.total_count) {
      break;
    }

    offset += page.records.length;
  }

  return all;
}
