<script lang="ts">
  import { goto } from "$app/navigation";
  import Button from "$lib/ui/Button.svelte";
  import FormField from "$lib/ui/FormField.svelte";
  import { login, register } from "./api";
  import { handleAuthSubmit } from "./submit";

  type Props = {
    mode: "login" | "register";
  };

  let { mode }: Props = $props();

  let username = $state("");
  let password = $state("");
  let usernameError = $state<string | null>(null);
  let passwordError = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let pending = $state(false);

  const isLogin = $derived(mode === "login");
  const title = $derived(isLogin ? "Sign in" : "Create account");
  const subtitle = $derived(isLogin ? "Access your ledger" : "Initialize a new ledger");
  const submitLabel = $derived(isLogin ? "Sign in" : "Create account");
  const pendingLabel = $derived(isLogin ? "Signing in…" : "Creating…");
  const alternateHref = $derived(isLogin ? "/register" : "/login");
  const alternateText = $derived(isLogin ? "Need an account? Register" : "Have an account? Sign in");
  const fallbackErrorMessage = $derived(isLogin ? "Unable to sign in." : "Unable to create account.");

  async function submit(event: SubmitEvent) {
    await handleAuthSubmit({
      event,
      username,
      password,
      onValidSubmit: async (u, p) => {
        const user = isLogin ? await login(u, p) : await register(u, p);
        if (user) {
          await goto("/home");
        }
      },
      setUsernameError: (message) => (usernameError = message),
      setPasswordError: (message) => (passwordError = message),
      setFormError: (message) => (formError = message),
      setPending: (value) => (pending = value),
      fallbackErrorMessage,
    });
  }
</script>

<section class="auth-shell" aria-labelledby="auth-title">
  <div class="auth-card">
    <header class="auth-header">
      <p class="eyebrow">Auth / Kash</p>
      <h1 id="auth-title">{title}</h1>
      <p class="subtitle">{subtitle}</p>
    </header>

    <form onsubmit={submit} novalidate>
      <FormField id="auth-username" label="Username" error={usernameError ?? undefined}>
        <input id="auth-username" type="text" bind:value={username} autocomplete="username" />
      </FormField>

      <FormField id="auth-password" label="Password" error={passwordError ?? undefined}>
        <input
          id="auth-password"
          type="password"
          bind:value={password}
          autocomplete={isLogin ? "current-password" : "new-password"}
        />
      </FormField>

      {#if formError}
        <p class="form-error" role="alert">{formError}</p>
      {/if}

      <Button variant="primary" type="submit" disabled={pending}>{pending ? pendingLabel : submitLabel}</Button>
    </form>

    <footer class="auth-footer">
      <a class="text-link" href={alternateHref}>{alternateText}</a>
    </footer>
  </div>
</section>

<style>
  .auth-shell {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: var(--space-8) var(--space-4);
  }

  .auth-card {
    width: min(100%, 420px);
    display: grid;
    gap: var(--space-6);
    padding: var(--space-6);
    border: 1px solid var(--border-strong);
    background: var(--panel);
  }

  .auth-header {
    display: grid;
    gap: var(--space-2);
    padding-bottom: var(--space-4);
    border-bottom: 1px solid var(--border);
  }

  .eyebrow,
  .subtitle,
  .auth-footer {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .auth-card form :global(input) {
    min-height: 42px;
    border-color: var(--border-strong);
    background: var(--surface);
  }

  .form-error {
    border: 1px solid var(--danger);
    padding: var(--space-3);
    background: var(--surface);
  }

  .auth-footer {
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }
</style>
