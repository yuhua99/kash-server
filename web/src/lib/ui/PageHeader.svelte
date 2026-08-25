<script lang="ts">
  type Props = {
    title: string;
  };

  let { title }: Props = $props();
</script>

<h1>{title}</h1>
