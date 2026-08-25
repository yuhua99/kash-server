<script lang="ts">
  import type { components } from "$lib/api/schema";
  import { validateSearchTerm } from "$lib/validation";
  import FormField from "$lib/ui/FormField.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import PeriodControls from "$lib/features/periods/PeriodControls.svelte";
  import type { PeriodPreset } from "$lib/date";
  import type { CategoryFilterMode, SortMode } from "$lib/features/records/view";

  type Category = components["schemas"]["Category"];

  type Props = {
    preset: PeriodPreset;
    start: string;
    end: string;
    search: string;
    categoryFilter: CategoryFilterMode;
    sortMode: SortMode;
    categories: Category[];
    onPeriodChange: (value: { preset: PeriodPreset; start: string; end: string }) => void;
    onSearchChange: (value: string) => void;
    onCategoryFilterChange: (value: CategoryFilterMode) => void;
    onSortChange: (value: SortMode) => void;
  };

  let {
    preset,
    start,
    end,
    search,
    categoryFilter,
    sortMode,
    categories,
    onPeriodChange,
    onSearchChange,
    onCategoryFilterChange,
    onSortChange,
  }: Props = $props();

  let searchError = $state("");

  const categoryItems = $derived([
    { value: "all_expenses", label: "All expenses" },
    { value: "all_incomes", label: "All incomes" },
    { kind: "separator" as const },
    ...categories.map((category) => ({ value: `category:${category.id}`, label: category.name })),
  ]);

  const sortItems = [
    { value: "date_desc", label: "Newest first" },
    { value: "date_asc", label: "Oldest first" },
    { value: "amount_desc", label: "Largest first" },
    { value: "amount_asc", label: "Smallest first" },
  ];

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    const error = validateSearchTerm(value);
    searchError = error ?? "";
    if (!error) {
      onSearchChange(value);
    }
  }
</script>

<div class="filters">
  <PeriodControls {preset} {start} {end} {onPeriodChange} />

  <FormField id="record-search" label="Search" error={searchError}>
    <input
      id="record-search"
      type="search"
      value={search}
      autocomplete="off"
      oninput={handleSearchInput}
    />
  </FormField>

  <div class="filters__selects">
    <SelectField
      id="record-category-filter"
      label="Category"
      value={categoryFilter}
      items={categoryItems}
      onValueChange={(value) => onCategoryFilterChange(value as CategoryFilterMode)}
    />
    <SelectField
      id="record-sort"
      label="Sort"
      value={sortMode}
      items={sortItems}
      onValueChange={(value) => onSortChange(value as SortMode)}
    />
  </div>
</div>

<style>
  .filters {
    display: grid;
    gap: var(--space-3);
  }

  .filters__selects {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  @media (max-width: 520px) {
    .filters__selects {
      grid-template-columns: 1fr;
    }
  }
</style>
