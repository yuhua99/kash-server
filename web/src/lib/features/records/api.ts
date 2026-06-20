import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type RecordItem = components["schemas"]["Record"];
type GetRecordsResponse = components["schemas"]["GetRecordsResponse"];
type CreateRecordPayload = components["schemas"]["CreateRecordPayload"];
type UpdateRecordPayload = components["schemas"]["UpdateRecordPayload"];

export function getRecords(params: {
  start_date?: string;
  end_date?: string;
  limit?: number;
  offset?: number;
}): Promise<GetRecordsResponse> {
  return client.get<GetRecordsResponse>("/records", {
    start_date: params.start_date,
    end_date: params.end_date,
    limit: params.limit,
    offset: params.offset,
  });
}

export function createRecord(body: CreateRecordPayload): Promise<RecordItem> {
  return client.post<RecordItem>("/records", body);
}

export function updateRecord(id: string, body: UpdateRecordPayload): Promise<RecordItem> {
  return client.put<RecordItem>(`/records/${id}`, body);
}

export function deleteRecord(id: string): Promise<void> {
  return client.del<void>(`/records/${id}`);
}
