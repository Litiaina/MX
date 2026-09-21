<script lang="ts">
  import { onMount } from 'svelte';
  import {
    authenticate,
    bootstrapAdministrator,
    bootstrapRequired
  } from '../api/auth';
  import type { DeploymentConfig } from '../api/domain';

  let {
    message = '',
    onAuthenticated,
    deployment
  }: {
    message?: string;
    onAuthenticated: () => Promise<void>;
    deployment: DeploymentConfig;
  } = $props();

  let mode = $state<'loading' | 'login' | 'bootstrap'>('loading');
  let busy = $state(false);
  let status = $state('');
  let statusIsError = $state(false);
  let email = $state('');
  let password = $state('');
  let factor = $state('');
  let authKey = $state('');
  let name = $state('');
  let confirmPassword = $state('');
  let failedLogoUrl = $state('');

  const brandName = $derived(deployment.branding.display_name.trim() || 'MX');
  const brandSubtitle = $derived(deployment.branding.subtitle.trim());
  const organizationName = $derived(deployment.branding.organization_name.trim());
  const logoUrl = $derived(deployment.branding.logo_url.trim());
  const showLogo = $derived(Boolean(logoUrl) && failedLogoUrl !== logoUrl);
  const brandInitials = $derived(
    brandName.split(/\s+/).map((part) => part[0]).join('').slice(0, 2).toUpperCase()
  );
  const feedbackTone = $derived(
    status
      ? (statusIsError ? 'error' : 'neutral')
      : (message.toLowerCase().includes('signed out') ? 'success' : 'error')
  );

  onMount(async () => {
    try {
      mode = (await bootstrapRequired()) ? 'bootstrap' : 'login';
    } catch (error) {
      mode = 'login';
      statusIsError = true;
      status = error instanceof Error ? error.message : 'Unable to contact MX.';
    }
  });

  async function submitLogin(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    statusIsError = false;
    status = 'Signing in…';
    try {
      await authenticate(email, password, factor);
      password = '';
      factor = '';
      status = '';
      await onAuthenticated();
    } catch (error) {
      statusIsError = true;
      status = error instanceof Error ? error.message : 'Authentication failed.';
    } finally {
      busy = false;
    }
  }

  async function submitBootstrap(event: SubmitEvent) {
    event.preventDefault();
    if (password !== confirmPassword) {
      statusIsError = true;
      status = 'Passwords do not match.';
      return;
    }

    busy = true;
    statusIsError = false;
    status = 'Creating the first Administrator…';
    try {
      await bootstrapAdministrator({ authKey, name: name.trim(), email: email.trim(), password });
      await authenticate(email, password, '');
      authKey = '';
      password = '';
      confirmPassword = '';
      status = '';
      await onAuthenticated();
    } catch (error) {
      statusIsError = true;
      status = error instanceof Error ? error.message : 'Administrator creation failed.';
    } finally {
      busy = false;
    }
  }
</script>

<main class="auth-page">
  <section class="auth-shell">
    <aside class="auth-identity" aria-label={`${brandName} identity`}>
      <div>
        <div class="auth-logo">
          {#if showLogo}
            <img src={logoUrl} alt={`${brandName} logo`} onerror={() => failedLogoUrl = logoUrl} />
          {:else}
            <span aria-hidden="true">{brandInitials}</span>
          {/if}
        </div>
        {#if organizationName}<p class="auth-organization">{organizationName}</p>{/if}
        <h1>{brandName}</h1>
        {#if brandSubtitle}<p class="auth-subtitle">{brandSubtitle}</p>{/if}
      </div>
      <div class="auth-assurance">
        <span aria-hidden="true">✓</span>
        <div><strong>Protected workspace</strong><small>Your account and security settings stay under your control.</small></div>
      </div>
    </aside>

    <section class="auth-form-panel" aria-labelledby="auth-title">
      {#if mode === 'loading'}
        <div class="auth-loading" aria-busy="true">
          <div class="loader"></div>
          <div><h2 id="auth-title">Preparing your workspace</h2><p class="muted">Checking this MX deployment…</p></div>
        </div>
      {:else if mode === 'bootstrap'}
        <header class="auth-form-header">
          <p class="auth-kicker">New deployment</p>
          <h2 id="auth-title">Create the first Administrator</h2>
          <p class="muted">Initialize this deployment with its permanent administrative account.</p>
        </header>
        <form class="form-stack auth-form" onsubmit={submitBootstrap}>
          <label>Bootstrap authorization key<input bind:value={authKey} type="password" autocomplete="off" required /></label>
          <label>Name<input bind:value={name} autocomplete="name" required /></label>
          <label>Email<input bind:value={email} type="email" autocomplete="username" required /></label>
          <label>Password<input bind:value={password} type="password" minlength="12" autocomplete="new-password" required /></label>
          <label>Confirm password<input bind:value={confirmPassword} type="password" minlength="12" autocomplete="new-password" required /></label>
          <button class="button primary" type="submit" disabled={busy}>{busy ? 'Creating Administrator…' : 'Create Administrator'}</button>
        </form>
      {:else}
        <header class="auth-form-header">
          <p class="auth-kicker">Secure access</p>
          <h2 id="auth-title">Welcome back</h2>
          <p class="muted">Sign in to continue to {brandName}.</p>
        </header>
        <form class="form-stack auth-form" onsubmit={submitLogin}>
          <label>Email<input bind:value={email} type="email" autocomplete="username" required /></label>
          <label>Password<input bind:value={password} type="password" autocomplete="current-password" required /></label>
          <label>
            Authenticator or recovery code
            <input bind:value={factor} autocomplete="one-time-code" inputmode="text" spellcheck={false} placeholder="6-digit or recovery code" />
            <small>Only required when two-factor authentication is enabled.</small>
          </label>
          <button class="button primary" type="submit" disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button>
        </form>
      {/if}

      {#if status || message}
        <p class:error={feedbackTone === 'error'} class:success={feedbackTone === 'success'} class="auth-status" role="status" aria-live="polite">{status || message}</p>
      {/if}
    </section>
  </section>
</main>
