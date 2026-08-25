<script lang="ts">
  type Props = {
    kind: "loading" | "error";
    message: string;
  };

  let { kind, message }: Props = $props();
</script>

<p role={kind === "loading" ? "status" : "alert"}>{message}</p>
