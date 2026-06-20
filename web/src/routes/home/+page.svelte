<script lang="ts">
  import QuickAddForm from "$lib/features/records/QuickAddForm.svelte";
</script>

<section class="home">
  <header class="home__header">
    <h1>Add</h1>
  </header>
  <QuickAddForm />
</section>

<style>
  .home {
    display: grid;
    gap: var(--space-6);
  }
</style>
