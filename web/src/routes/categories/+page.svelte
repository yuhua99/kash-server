<script lang="ts">
  import { onMount } from "svelte";
  import type { components } from "$lib/api/schema";
  import CategoryForm from "$lib/features/categories/CategoryForm.svelte";
  import CategoryList from "$lib/features/categories/CategoryList.svelte";
  import { getCategoriesCached } from "$lib/features/categories/cache";

  type Category = components["schemas"]["Category"];

  let categories = $state<Category[]>([]);
  let loading = $state(true);
  let error = $state("");

  async function refresh() {
    loading = true;
    error = "";
    try {
      categories = await getCategoriesCached();
    } catch {
      error = "Could not load categories.";
    } finally {
      loading = false;
    }
  }

  onMount(refresh);
</script>

<section class="page">
  <h1>Categories</h1>

  <CategoryForm onChange={refresh} />

  {#if loading}
    <p role="status">Loading…</p>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <CategoryList {categories} onChange={refresh} />
  {/if}
</section>

<style>
  .page {
    display: grid;
    gap: var(--space-4);
  }
</style>
