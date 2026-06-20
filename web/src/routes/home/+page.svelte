<script lang="ts">
  import QuickAddForm from "$lib/features/records/QuickAddForm.svelte";
</script>

<section class="home">
  <header class="home__header">
    <p class="home__eyebrow">Unit / Kash</p>
    <h1>Add</h1>
  </header>
  <QuickAddForm />
</section>

<style>
  .home {
    display: grid;
    gap: var(--space-6);
  }

  .home__eyebrow {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  h1 {
    font-family: var(--font-display);
    font-size: clamp(2.5rem, 12vw, 4.5rem);
    font-weight: 900;
    letter-spacing: -0.05em;
    line-height: 0.9;
    text-transform: uppercase;
  }
</style>
