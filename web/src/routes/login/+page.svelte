<script lang="ts">
  import AuthForm from "$lib/features/auth/AuthForm.svelte";
  import PageHeader from "$lib/ui/PageHeader.svelte";
</script>

<section class="page">
  <PageHeader title="Sign in" />
  <AuthForm mode="login" />
</section>
