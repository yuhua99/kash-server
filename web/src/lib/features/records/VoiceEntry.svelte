<script lang="ts">
  import { onMount } from "svelte";
  import type { components } from "$lib/api/schema";
  import { todayIso } from "$lib/date";
  import { handleApiError } from "$lib/api/errors";
  import Button from "$lib/ui/Button.svelte";
  import StatusMessage from "$lib/ui/StatusMessage.svelte";
  import { createVoiceDraft } from "./api";
  import { supportsVoiceRecording, VoiceRecorder, type RecordingState } from "./voice-recorder";

  type Props = {
    disabled: boolean;
    currency: string;
    onBusyChange: (busy: boolean) => void;
    onDraft: (draft: components["schemas"]["VoiceEntryResponse"]) => void;
  };
  let { disabled, currency, onBusyChange, onDraft }: Props = $props();
  let recordingState = $state<RecordingState>("idle");
  let supported = $state(true);
  let error = $state("");
  let transcript = $state("");
  let recorder: VoiceRecorder | undefined;
  let abort: AbortController | undefined;
  let recordingCurrency = "";
  let referenceDate = "";

  onMount(() => {
    let disposed = false;
    supported = supportsVoiceRecording();
    recorder = new VoiceRecorder(
      (next) => {
        recordingState = next;
        onBusyChange(next !== "idle");
      },
      async (audio) => {
        abort = new AbortController();
        try {
          const draft = await createVoiceDraft(audio, referenceDate, recordingCurrency, abort.signal);
          if (!disposed) {
            onDraft(draft);
            transcript = draft.transcript;
          }
        } catch (cause) {
          if (!disposed) {
            const message = await handleApiError(cause, "Could not process voice entry");
            if (message) error = message;
          }
        }
      },
      (message) => (error = message),
    );
    return () => {
      disposed = true;
      recorder?.dispose();
      abort?.abort();
    };
  });

  function toggle() {
    if (recordingState === "recording") {
      recorder?.stop();
    } else if (recordingState === "idle") {
      error = "";
      transcript = "";
      recordingCurrency = currency;
      referenceDate = todayIso();
      void recorder?.start();
    }
  }
</script>

<div class="voice-entry">
  <Button
    variant="secondary"
    disabled={!supported || (disabled && recordingState === "idle")}
    busy={recordingState === "requesting" || recordingState === "processing"}
    busyLabel={recordingState === "requesting" ? "Allow microphone" : "Processing voice"}
    onclick={toggle}
  >
    {recordingState === "recording" ? "Stop recording" : "Voice entry"}
  </Button>
  {#if !supported}
    <p class="meta-label">Voice entry requires HTTPS and a browser with microphone recording support.</p>
  {/if}
  {#if recordingState === "recording"}
    <p class="meta-label" role="status">Recording. Click stop when finished (maximum 60 seconds).</p>
  {/if}
  {#if transcript}
    <p class="voice-transcript" role="status">{transcript}</p>
  {/if}
  {#if error}
    <StatusMessage kind="error" message={error} />
  {/if}
</div>

<style>
  .voice-entry {
    display: grid;
    gap: var(--space-2);
  }

  .voice-transcript {
    overflow-wrap: anywhere;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }
</style>
