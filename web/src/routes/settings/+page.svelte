<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { handleApiError } from "$lib/api/errors";
  import Block from "$lib/ui/Block.svelte";
  import Button from "$lib/ui/Button.svelte";
  import SelectField from "$lib/ui/SelectField.svelte";
  import { toast } from "$lib/ui/toast";
  import { logout } from "$lib/features/auth/api";
  import {
    amountDisplayMode,
    setAmountDisplayMode,
    type AmountDisplayMode,
  } from "$lib/features/money/amount-display";
  import { SUPPORTED_CURRENCIES, type SupportedCurrencyCode } from "$lib/features/money/currency";
  import { currentCurrency, setCurrentCurrency } from "$lib/features/money/current-currency";
  import { updateSettings } from "$lib/features/settings/api";
  import { getSettingsCached, invalidateSettingsCache, setSettingsCache } from "$lib/features/settings/cache";
  import { invalidateCategoriesCache } from "$lib/features/categories/cache";
  import { invalidateFriendsCache } from "$lib/features/friends/cache";

  let mainCurrency = $state("");
  let saving = $state(false);

  const currencyItems = SUPPORTED_CURRENCIES.map((code) => ({ value: code, label: code }));
  const amountItems = [
    { value: "cents", label: "Cents" },
    { value: "whole", label: "Whole" },
  ];

  onMount(async () => {
    try {
      mainCurrency = (await getSettingsCached()).main_currency;
    } catch {
      mainCurrency = "";
    }
  });

  async function changeMainCurrency(value: string) {
    mainCurrency = value;
    saving = true;
    try {
      const updated = await updateSettings(value);
      setSettingsCache(updated);
      toast.success("Main currency updated");
    } catch (e) {
      const message = await handleApiError(e, "Could not update currency");
      if (message) {
        toast.error(message);
      }
    } finally {
      saving = false;
    }
  }

  async function signOut() {
    try {
      await logout();
    } catch {
      // ignore; navigate regardless
    }
    invalidateSettingsCache();
    invalidateCategoriesCache();
    invalidateFriendsCache();
    await goto("/login");
  }
</script>

<section class="page">
  <h1>Settings</h1>

  <Block title="Currency">
    <div class="stack">
      <SelectField
        id="settings-main-currency"
        label="Main currency"
        value={mainCurrency}
        items={currencyItems}
        disabled={saving}
        onValueChange={changeMainCurrency}
      />
      <SelectField
        id="settings-current-currency"
        label="Entry currency"
        value={$currentCurrency}
        items={currencyItems}
        onValueChange={(value) => setCurrentCurrency(value as SupportedCurrencyCode)}
      />
    </div>
  </Block>

  <Block title="Display">
    <SelectField
      id="settings-amount-display"
      label="Amount display"
      value={$amountDisplayMode}
      items={amountItems}
      onValueChange={(value) => setAmountDisplayMode(value as AmountDisplayMode)}
    />
  </Block>

  <Block title="Account">
    <div class="stack">
      <a class="text-link" href="/settings/friends">Manage friends</a>
      <Button variant="secondary" className="signout" onclick={signOut}>Sign out</Button>
    </div>
  </Block>
</section>

<style>
  .page {
    display: grid;
    gap: var(--space-4);
  }

  .stack {
    display: grid;
    gap: var(--space-3);
  }

  h1 {
    font-family: var(--font-display);
    font-size: clamp(2rem, 9vw, 3.5rem);
    font-weight: 900;
    letter-spacing: -0.04em;
    text-transform: uppercase;
  }

  :global(.signout) {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
