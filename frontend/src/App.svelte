<script lang="ts">
  import { onMount } from 'svelte';
  import LoginPanel from './lib/components/LoginPanel.svelte';
  import Workspace from './lib/components/Workspace.svelte';
  import { AUTH_EXPIRED_EVENT } from './lib/api/client';
  import type { DeploymentConfig } from './lib/api/domain';
  import { loadDeployment } from './lib/api/workspace';
  import { currentSession, endSession, reloadSession, restoreSession } from './lib/auth/session';

  let loading = $state(true);
  let authMessage = $state('');
  const defaults: DeploymentConfig = {
    branding: { display_name: 'MX', subtitle: "Litiaina's General-Purpose System", organization_name: '', logo_url: 'images/system-icon.png' },
    appearance: { preset: 'blue', primary_color: '#1d4ed8', sidebar_color: '#0f172a', radius: 'rounded', density: 'normal', default_theme: 'light', content_width: 'wide' },
    terminology: { record_singular: 'Record', record_plural: 'Records', dashboard_label: 'Dashboard', administration_label: 'Administration' },
    navigation: { show_dashboard: true, show_records: true, show_quick_actions: true, default_workspace: 'dashboard' }
  };
  let deployment = $state<DeploymentConfig>(defaults);

  onMount(() => {
    const handleAuthExpired = (event: Event) => {
      const detail = event instanceof CustomEvent ? event.detail as { message?: string } : null;
      sessionEnded(detail?.message || 'Your session has expired. Sign in again.');
    };
    window.addEventListener(AUTH_EXPIRED_EVENT, handleAuthExpired);

    void Promise.allSettled([restoreSession(), reloadDeployment()]).finally(() => {
      loading = false;
    });

    return () => window.removeEventListener(AUTH_EXPIRED_EVENT, handleAuthExpired);
  });

  $effect(() => {
    document.documentElement.style.setProperty('--primary', deployment.appearance.primary_color);
    document.documentElement.style.setProperty('--sidebar', deployment.appearance.sidebar_color);
    const storedTheme = localStorage.getItem('mx_theme');
    document.documentElement.dataset.theme = storedTheme || deployment.appearance.default_theme;
    document.documentElement.dataset.density = deployment.appearance.density;
    document.documentElement.style.setProperty('--corner-radius', ({ square: '3px', subtle: '7px', rounded: '12px', soft: '18px' } as Record<string, string>)[deployment.appearance.radius] || '12px');
    document.documentElement.style.setProperty('--content-max', ({ standard: '76rem', wide: '92rem', full: '120rem' } as Record<string, string>)[deployment.appearance.content_width] || '92rem');
    document.title = deployment.branding.display_name.trim() || 'MX';
    const favicon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    const fallbackIcon = '/images/system-icon.png';
    const configuredIcon = deployment.branding.logo_url.trim() || fallbackIcon;
    if (favicon) {
      favicon.onerror = configuredIcon === fallbackIcon ? null : () => {
        favicon.onerror = null;
        favicon.href = fallbackIcon;
      };
      favicon.href = configuredIcon;
    }
    const themeColor = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
    if (themeColor) themeColor.content = deployment.appearance.sidebar_color;
  });

  async function reloadDeployment() {
    try { deployment = (await loadDeployment()).config; }
    catch { deployment = defaults; }
  }

  async function authenticated() {
    await reloadSession();
    authMessage = '';
  }

  function sessionEnded(message: string) {
    endSession();
    authMessage = message;
  }
</script>

{#if loading}
  <main class="loading-page" aria-busy="true">
    <div class="loader"></div>
    <p>Loading MX…</p>
  </main>
{:else if !$currentSession}
  <LoginPanel message={authMessage} onAuthenticated={authenticated} {deployment} />
{:else}
  <Workspace session={$currentSession} {deployment} onSessionChanged={reloadSession} onSessionEnded={sessionEnded} onDeploymentChanged={reloadDeployment} />
{/if}
