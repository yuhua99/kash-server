<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import Button from "./Button.svelte";
  import ButtonRow from "./ButtonRow.svelte";

  type Props = {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    title: string;
    description: string;
    confirmLabel: string;
    confirmBusyLabel: string;
    busy?: boolean;
    onConfirm: () => void;
  };

  let {
    open = $bindable(false),
    onOpenChange,
    title,
    description,
    confirmLabel,
    confirmBusyLabel,
    busy = false,
    onConfirm,
  }: Props = $props();

  function closeDialog() {
    open = false;
    onOpenChange?.(false);
  }
</script>

<Dialog bind:open {onOpenChange} {title} {description}>
  <ButtonRow>
    <Button variant="secondary" type="button" onclick={closeDialog}>Cancel</Button>
    <Button variant="primary" type="button" disabled={busy} onclick={onConfirm} className="btn-danger">
      {busy ? confirmBusyLabel : confirmLabel}
    </Button>
  </ButtonRow>
</Dialog>

<style>
  :global(.btn-danger) {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
