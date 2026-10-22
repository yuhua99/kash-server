import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { supportsVoiceRecording, VoiceRecorder } from "./voice-recorder";

class MockMediaRecorder {
  static isTypeSupported = vi.fn((type: string) => type.startsWith("audio/webm"));
  static instances: MockMediaRecorder[] = [];
  state = "inactive";
  mimeType: string;
  ondataavailable?: (event: { data: Blob }) => void;
  onstop?: () => void;
  onerror?: () => void;

  constructor(_stream: MediaStream, options: MediaRecorderOptions) {
    this.mimeType = options.mimeType!;
    MockMediaRecorder.instances.push(this);
  }
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob(["recorded audio"]) });
    this.onstop?.();
  }
}

let stopTrack: ReturnType<typeof vi.fn>;
let getUserMedia: ReturnType<typeof vi.fn>;
const stream = () => ({ getTracks: () => [{ stop: stopTrack }] }) as unknown as MediaStream;

beforeEach(() => {
  vi.useFakeTimers();
  stopTrack = vi.fn();
  getUserMedia = vi.fn().mockImplementation(async () => stream());
  MockMediaRecorder.instances = [];
  MockMediaRecorder.isTypeSupported.mockImplementation((type) => type.startsWith("audio/webm"));
  vi.stubGlobal("MediaRecorder", MockMediaRecorder);
  vi.stubGlobal("navigator", { mediaDevices: { getUserMedia } });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("VoiceRecorder", () => {
  it("records, uploads once on stop, and releases the microphone", async () => {
    const states = vi.fn();
    const recorded = vi.fn().mockResolvedValue(undefined);
    const errors = vi.fn();
    const recorder = new VoiceRecorder(states, recorded, errors);
    await recorder.start();
    await recorder.start();
    expect(getUserMedia).toHaveBeenCalledTimes(1);
    recorder.stop();
    await Promise.resolve();
    expect(recorded).toHaveBeenCalledTimes(1);
    expect(recorded.mock.calls[0][0].type).toBe("audio/webm;codecs=opus");
    expect(stopTrack).toHaveBeenCalled();
    expect(states.mock.calls.map((call) => call[0])).toEqual([
      "requesting",
      "recording",
      "processing",
      "processing",
      "idle",
    ]);
    expect(errors).not.toHaveBeenCalled();
    recorder.dispose();
  });

  it("recovers from denied microphone permissions", async () => {
    getUserMedia.mockRejectedValue(new Error("denied"));
    const errors = vi.fn();
    const states = vi.fn();
    await new VoiceRecorder(states, vi.fn(), errors).start();
    expect(errors).toHaveBeenCalledWith(expect.stringContaining("Allow microphone"));
    expect(states).toHaveBeenLastCalledWith("idle");
  });

  it("stops tracks if permission arrives after unmount and never uploads", async () => {
    let resolve!: (value: MediaStream) => void;
    getUserMedia.mockReturnValue(new Promise<MediaStream>((r) => (resolve = r)));
    const recorded = vi.fn();
    const recorder = new VoiceRecorder(vi.fn(), recorded, vi.fn());
    const start = recorder.start();
    recorder.dispose();
    resolve(stream());
    await start;
    expect(stopTrack).toHaveBeenCalledTimes(1);
    expect(recorded).not.toHaveBeenCalled();
  });

  it("does not upload a recording when disposed", async () => {
    const recorded = vi.fn();
    const recorder = new VoiceRecorder(vi.fn(), recorded, vi.fn());
    await recorder.start();
    recorder.dispose();
    expect(recorded).not.toHaveBeenCalled();
    expect(stopTrack).toHaveBeenCalled();
  });

  it("automatically stops after 60 seconds", async () => {
    const recorded = vi.fn().mockResolvedValue(undefined);
    const recorder = new VoiceRecorder(vi.fn(), recorded, vi.fn());
    await recorder.start();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(recorded).toHaveBeenCalledTimes(1);
    expect(stopTrack).toHaveBeenCalled();
    recorder.dispose();
  });

  it("handles recording failure without uploading", async () => {
    const errors = vi.fn();
    const recorded = vi.fn();
    const recorder = new VoiceRecorder(vi.fn(), recorded, errors);
    await recorder.start();
    MockMediaRecorder.instances[0].onerror?.();
    expect(recorded).not.toHaveBeenCalled();
    expect(errors).toHaveBeenCalledWith(expect.stringContaining("recording failed"));
    expect(stopTrack).toHaveBeenCalled();
  });

  it("rejects oversized audio", async () => {
    const errors = vi.fn();
    const recorded = vi.fn();
    const recorder = new VoiceRecorder(vi.fn(), recorded, errors);
    await recorder.start();
    MockMediaRecorder.instances[0].ondataavailable?.({
      data: new Blob([new Uint8Array(10 * 1024 * 1024)]),
    });
    expect(errors).toHaveBeenCalledWith(expect.stringContaining("too large"));
    expect(recorded).not.toHaveBeenCalled();
  });

  it("reports unsupported or insecure browsers without requesting permission", async () => {
    vi.stubGlobal("navigator", {});
    expect(supportsVoiceRecording()).toBe(false);
    const errors = vi.fn();
    await new VoiceRecorder(vi.fn(), vi.fn(), errors).start();
    expect(errors).toHaveBeenCalledWith(expect.stringContaining("HTTPS"));
    expect(getUserMedia).not.toHaveBeenCalled();
  });

  it("uses MP4 when WebM is not supported", async () => {
    MockMediaRecorder.isTypeSupported.mockImplementation((type) => type === "audio/mp4");
    const recorded = vi.fn().mockResolvedValue(undefined);
    const recorder = new VoiceRecorder(vi.fn(), recorded, vi.fn());
    await recorder.start();
    recorder.stop();
    await Promise.resolve();
    expect(recorded.mock.calls[0][0].type).toBe("audio/mp4");
    recorder.dispose();
  });
});
