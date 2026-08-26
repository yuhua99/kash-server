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
  let pending = $state(false);

  const isLogin = $derived(mode === "login");
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
      setPending: (value) => (pending = value),
      fallbackErrorMessage,
    });
  }
</script>

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

  <Button variant="primary" type="submit" disabled={pending}>{pending ? pendingLabel : submitLabel}</Button>
</form>

<a class="text-link" href={alternateHref}>{alternateText}</a>
