import { afterEach, describe, expect, it, vi } from "vitest";
import { client } from "./client";

afterEach(() => vi.unstubAllGlobals());

describe("API request bodies", () => {
  it("preserves JSON request encoding", async () => {
    const fetch = vi.fn().mockResolvedValue(new Response('{"ok":true}'));
    vi.stubGlobal("fetch", fetch);
    await client.post("/records", { name: "lunch" });
    expect(fetch).toHaveBeenCalledWith(
      "/api/records",
      expect.objectContaining({
        body: '{"name":"lunch"}',
        headers: { "Content-Type": "application/json" },
        credentials: "include",
      }),
    );
  });

  it("sends FormData without setting a multipart boundary and supports cancellation", async () => {
    const fetch = vi.fn().mockResolvedValue(new Response('{"name":"lunch"}'));
    vi.stubGlobal("fetch", fetch);
    const form = new FormData();
    form.append("audio", new Blob(["audio"], { type: "audio/webm" }), "voice.webm");
    const abort = new AbortController();
    await client.post("/records/voice", form, abort.signal);
    expect(fetch).toHaveBeenCalledWith(
      "/api/records/voice",
      expect.objectContaining({
        body: form,
        headers: {},
        signal: abort.signal,
      }),
    );
  });
});
