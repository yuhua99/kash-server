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

export function createVoiceDraft(
  audio: Blob,
  referenceDate: string,
  currency: string,
  signal?: AbortSignal,
): Promise<components["schemas"]["VoiceEntryResponse"]> {
  const body = new FormData();
  body.append("audio", audio, audio.type.startsWith("audio/mp4") ? "voice.mp4" : "voice.webm");
  body.append("reference_date", referenceDate);
  body.append("currency", currency);
  return client.post<components["schemas"]["VoiceEntryResponse"]>("/records/voice", body, signal);
}

export function updateRecord(id: string, body: UpdateRecordPayload): Promise<RecordItem> {
  return client.put<RecordItem>(`/records/${id}`, body);
}

export function deleteRecord(id: string): Promise<void> {
  return client.del<void>(`/records/${id}`);
}
