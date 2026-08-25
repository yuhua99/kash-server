<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import ToastHost from "$lib/ui/ToastHost.svelte";
  import PendingInbox from "$lib/features/inbox/PendingInbox.svelte";
  import { initializeCurrentCurrency } from "$lib/features/money/current-currency";
  import { getSettingsCached } from "$lib/features/settings/cache";
  import { isActive, navItems } from "$lib/features/shell/nav";
  import { registerServiceWorker } from "$lib/features/shell/service-worker";

  let { data, children } = $props();

  const user = $derived(data.user);
  const pathname = $derived(page.url.pathname);
  const isAuthRoute = $derived(pathname === "/login" || pathname === "/register");
  const showShell = $derived(Boolean(user) && !isAuthRoute);

  let initializedFor = $state<string | null>(null);

  $effect(() => {
    if (user && initializedFor !== user.id) {
      initializedFor = user.id;
      getSettingsCached()
        .then((s) => initializeCurrentCurrency(s.main_currency))
        .catch(() => initializeCurrentCurrency(""));
    } else if (!user && initializedFor !== null) {
      initializedFor = null;
    }
  });

  onMount(() => registerServiceWorker());
</script>

<ToastHost />

{#if user && !isAuthRoute}
  <PendingInbox userId={user.id} />
{/if}

<div class="app" class:app--shell={showShell}>
  <main>
    {@render children?.()}
  </main>

  {#if showShell}
    <nav class="nav" aria-label="Primary">
      {#each navItems as item (item.href)}
        <a
          class="nav__link"
          class:nav__link--active={isActive(pathname, item.href)}
          href={item.href}
          aria-current={isActive(pathname, item.href) ? "page" : undefined}
        >
          {item.label}
        </a>
      {/each}
    </nav>
  {/if}
</div>

<style>
  .app {
    min-height: 100dvh;
  }

  .app--shell {
    padding-bottom: calc(64px + env(safe-area-inset-bottom));
  }

  .nav {
    position: fixed;
    inset-inline: 0;
    bottom: 0;
    z-index: 40;
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 1px;
    border-top: 1px solid var(--border-strong);
    background: var(--border);
    padding-bottom: env(safe-area-inset-bottom);
  }

  .nav__link {
    display: grid;
    place-items: center;
    min-height: 56px;
    padding: var(--space-2);
    background: var(--panel);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-align: center;
    text-transform: uppercase;
  }

  .nav__link--active {
    background: var(--surface);
    color: var(--accent);
  }
</style>
