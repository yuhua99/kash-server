export type RecordingState = "idle" | "requesting" | "recording" | "processing";

const MAX_RECORDING_MS = 60_000;
const MAX_AUDIO_BYTES = 10 * 1024 * 1024 - 4096;
const MIME_TYPES = ["audio/webm;codecs=opus", "audio/mp4"];

export function supportsVoiceRecording(): boolean {
  return (
    typeof MediaRecorder !== "undefined" &&
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices?.getUserMedia &&
    MIME_TYPES.some((type) => MediaRecorder.isTypeSupported(type))
  );
}

export class VoiceRecorder {
  private recorder?: MediaRecorder;
  private stream?: MediaStream;
  private timer?: ReturnType<typeof setTimeout>;
  private disposed = false;
  private active = false;
  private failed = false;

  constructor(
    private readonly onState: (state: RecordingState) => void,
    private readonly onRecorded: (audio: Blob) => Promise<void>,
    private readonly onError: (message: string) => void,
  ) {}

  async start(): Promise<void> {
    if (this.disposed || this.active) return;
    if (!supportsVoiceRecording()) {
      this.onError("Voice entry requires HTTPS and a browser with microphone recording support.");
      return;
    }
    this.active = true;
    this.failed = false;
    this.onState("requesting");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (this.disposed) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      this.stream = stream;
      const mimeType = MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type))!;
      const recorder = new MediaRecorder(stream, { mimeType });
      this.recorder = recorder;
      const chunks: Blob[] = [];
      let size = 0;
      recorder.ondataavailable = (event) => {
        if (this.disposed || this.failed) return;
        size += event.data.size;
        if (size > MAX_AUDIO_BYTES) {
          this.fail("Recording is too large. Please use a shorter entry.");
          return;
        }
        if (event.data.size) chunks.push(event.data);
      };
      recorder.onerror = () => this.fail("Microphone recording failed. Please try again.");
      recorder.onstop = () => {
        this.releaseMicrophone();
        if (this.disposed || this.failed) return;
        this.onState("processing");
        const audio = new Blob(chunks, { type: recorder.mimeType });
        void this.finish(audio);
      };
      recorder.start(1000);
      this.onState("recording");
      this.timer = setTimeout(() => this.stop(), MAX_RECORDING_MS);
    } catch {
      this.fail("Could not access the microphone. Allow microphone access and try again.");
    }
  }

  stop(): void {
    if (this.recorder?.state !== "recording") return;
    this.onState("processing");
    this.recorder.stop();
    this.releaseMicrophone();
  }

  dispose(): void {
    this.disposed = true;
    if (this.recorder?.state === "recording") this.recorder.stop();
    this.releaseMicrophone();
  }

  private async finish(audio: Blob): Promise<void> {
    try {
      if (!audio.size) throw new Error("No audio was recorded. Please try again.");
      await this.onRecorded(audio);
    } catch (error) {
      if (!this.disposed) {
        this.onError(error instanceof Error ? error.message : "Could not process voice entry.");
      }
    } finally {
      this.active = false;
      if (!this.disposed) this.onState("idle");
    }
  }

  private fail(message: string): void {
    this.failed = true;
    if (this.recorder?.state === "recording") this.recorder.stop();
    this.releaseMicrophone();
    this.active = false;
    if (!this.disposed) {
      this.onError(message);
      this.onState("idle");
    }
  }

  private releaseMicrophone(): void {
    clearTimeout(this.timer);
    this.stream?.getTracks().forEach((track) => track.stop());
    this.stream = undefined;
  }
}
