<script lang="ts">
  import { onMount } from 'svelte';
  import type { DeploymentConfig } from '../api/domain';
  import type { Session } from '../api/types';
  import { exportReport } from '../api/workspace';
  import { MxLiveClient, type LiveMessage } from '../live/client';
  import AccountSecurity from './AccountSecurity.svelte';
  import AdminView from './AdminView.svelte';
  import DashboardView from './DashboardView.svelte';
  import RecordsView from './RecordsView.svelte';

  let { session, deployment, onSessionChanged, onSessionEnded, onDeploymentChanged }:
    { session: Session; deployment: DeploymentConfig; onSessionChanged: () => Promise<unknown>; onSessionEnded: (message: string) => void; onDeploymentChanged: () => Promise<void> } = $props();
  type View = 'dashboard' | 'records' | 'admin' | 'account';
  let view = $state<View>('dashboard');
  let recordRevision = $state(0); let dashboardRevision = $state(0); let adminRevision = $state(0);
  let live = $state(false); let mobileOpen = $state(false); let settingsOpen = $state(false);
  let lastRecordEvent = $state<LiveMessage | null>(null);
  let autoScale = $state(true); let scale = $state(100); let autoRefresh = $state(0); let theme = $state('light');
  const viewTitle = $derived(view === 'dashboard' ? deployment.terminology.dashboard_label : view === 'records' ? deployment.terminology.record_plural : view === 'admin' ? deployment.terminology.administration_label : 'My account');

  onMount(() => {
    autoScale = localStorage.getItem('mx_ui_scale_auto') !== 'false';
    scale = Number(localStorage.getItem('mx_ui_scale_percent') || 100);
    autoRefresh = Number(localStorage.getItem('mx_auto_refresh_seconds') || 0);
    theme = document.documentElement.dataset.theme || deployment.appearance.default_theme;
    applyScale();
    const fromHash = hashView();
    view = fromHash || defaultView();
    if (!fromHash) history.replaceState(null, '', `#${view}`);
    const client = new MxLiveClient(handleLive, (connected) => {
      live = connected;
      if (connected) dashboardRevision += 1;
    });
    client.start();
    const handleHash = () => { const next = hashView(); if (next) { view = next; mobileOpen = false; } };
    const handleResize = () => { if (autoScale) applyScale(); };
    window.addEventListener('hashchange', handleHash);
    window.addEventListener('resize', handleResize);
    return () => { client.stop(); live = false; window.removeEventListener('hashchange', handleHash); window.removeEventListener('resize', handleResize); };
  });

  $effect(() => {
    if (!autoRefresh) return;
    const timer = window.setInterval(() => refreshCurrent(), Math.max(10, autoRefresh) * 1000);
    return () => window.clearInterval(timer);
  });

  function defaultView(): View {
    if (deployment.navigation.default_workspace === 'records' && deployment.navigation.show_records) return 'records';
    if (deployment.navigation.show_dashboard) return 'dashboard';
    return deployment.navigation.show_records ? 'records' : 'account';
  }
  function allowed(next: View) {
    if (next === 'dashboard') return deployment.navigation.show_dashboard;
    if (next === 'records') return deployment.navigation.show_records;
    if (next === 'admin') return session.access_level === 0;
    return true;
  }
  function hashView(): View | null { const candidate = location.hash.replace(/^#\/?/, '') as View; return ['dashboard', 'records', 'admin', 'account'].includes(candidate) && allowed(candidate) ? candidate : null; }
  function handleLive(message: LiveMessage) {
    if (message.type === 'presence.changed') { dashboardRevision += 1; return; }
    if (message.type === 'sync.required') {
      recordRevision += 1;
      dashboardRevision += 1;
      lastRecordEvent = message;
      return;
    }
    if (message.type.startsWith('record.') || message.type.startsWith('attachment.') || message.type.startsWith('schema.')) { recordRevision += 1; lastRecordEvent = message; }
    if (message.type.startsWith('record.') || message.type.startsWith('attachment.') || message.type === 'dashboard.updated') dashboardRevision += 1;
    if (message.type === 'deployment.updated') void onDeploymentChanged();
  }
  function go(next: View, replace = false) {
    if (!allowed(next)) return;
    view = next; mobileOpen = false;
    const url = `#${next}`;
    if (location.hash !== url) history[replace ? 'replaceState' : 'pushState'](null, '', url);
  }
  function refreshCurrent() {
    if (view === 'records') recordRevision += 1;
    else if (view === 'dashboard') dashboardRevision += 1;
    else if (view === 'admin') adminRevision += 1;
  }
  function initials(name: string) { return name.split(/\s+/).map((part) => part[0]).join('').slice(0, 2).toUpperCase(); }
  function setTheme(value: string) { theme = value; document.documentElement.dataset.theme = value; localStorage.setItem('mx_theme', value); }
  function toggleTheme() {
    const darkNow = document.documentElement.dataset.theme === 'dark' || (document.documentElement.dataset.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
    setTheme(darkNow ? 'light' : 'dark');
  }
  function recommendedScale() {
    const width = window.innerWidth;
    if (width < 480) return 90;
    if (width < 800) return 95;
    return 100;
  }
  function applyScale() {
    const value = autoScale ? recommendedScale() : Math.min(160, Math.max(85, Number(scale) || 100));
    document.documentElement.style.fontSize = `${value}%`;
  }
  function saveDisplaySettings() {
    scale = Math.min(160, Math.max(85, Number(scale) || 100));
    autoRefresh = Math.max(0, Number(autoRefresh) || 0);
    localStorage.setItem('mx_ui_scale_auto', String(autoScale));
    localStorage.setItem('mx_ui_scale_percent', String(scale));
    localStorage.setItem('mx_auto_refresh_seconds', String(autoRefresh));
    applyScale(); settingsOpen = false;
  }
</script>

<div class="application">
  <aside class:open={mobileOpen} class="sidebar">
    <div class="sidebar-brand">{#if deployment.branding.logo_url}<img src={deployment.branding.logo_url} alt="" />{:else}<span>{initials(deployment.branding.display_name)}</span>{/if}<div><strong>{deployment.branding.display_name}</strong><small>{deployment.branding.subtitle}</small></div></div>
    <nav class="main-nav" aria-label="Main navigation">
      {#if deployment.navigation.show_dashboard}<button class:active={view === 'dashboard'} onclick={() => go('dashboard')}><span>⌂</span>{deployment.terminology.dashboard_label}</button>{/if}
      {#if deployment.navigation.show_records}<button class:active={view === 'records'} onclick={() => go('records')}><span>▤</span>{deployment.terminology.record_plural}</button>{/if}
      {#if session.access_level === 0}<button class:active={view === 'admin'} onclick={() => go('admin')}><span>⚙</span>{deployment.terminology.administration_label}</button>{/if}
    </nav>
    {#if deployment.navigation.show_quick_actions}<section class="quick-actions"><span>Quick actions</span>{#if deployment.navigation.show_records}<button onclick={() => go('records')}><i>▤</i>Open {deployment.terminology.record_singular.toLowerCase()} workspace</button>{/if}<button onclick={() => exportReport('records')}><i>⇩</i>Export detailed {deployment.terminology.record_plural.toLowerCase()}</button></section>{/if}
    <div class="sidebar-foot"><div class="live-state"><i class:online={live}></i>{live ? 'Live sync connected' : 'Reconnecting live sync'}</div><button class:active={view === 'account'} class="profile-button" onclick={() => go('account')}><span>{initials(session.name)}</span><div><strong>{session.name}</strong><small>{session.access_name} · My account</small></div></button></div>
  </aside>
  <main class="main-stage">
    <header class="mobile-header"><button class="icon-button" aria-label="Open navigation" onclick={() => mobileOpen = !mobileOpen}>☰</button><strong>{viewTitle}</strong><button class="button small" onclick={refreshCurrent}>Refresh</button></header>
    <header class="workspace-topbar"><div class="topbar-context"><button class="menu-button" aria-label="Open navigation menu" aria-expanded={mobileOpen} onclick={() => mobileOpen = !mobileOpen}>☰</button><div class="workspace-identity"><small>{deployment.branding.display_name}</small><strong>{viewTitle}</strong></div></div><div class="desktop-actions"><span class:online={live} class="topbar-live">{live ? 'Live' : 'Reconnecting'}</span><button class="link-button" onclick={refreshCurrent}>Refresh</button><button class="link-button" onclick={() => settingsOpen = true}>Settings</button><button class="link-button" onclick={toggleTheme}>Theme</button><button class="link-button" onclick={() => go('account')}>My account</button><button class="button small sign-out-button" onclick={() => onSessionEnded('Signed out.')}>Sign out</button></div></header>
    <nav class="workspace-tabs" aria-label="Workspace tabs">{#if deployment.navigation.show_dashboard}<button class:active={view === 'dashboard'} onclick={() => go('dashboard')}><span>⌂</span>{deployment.terminology.dashboard_label}</button>{/if}{#if deployment.navigation.show_records}<button class:active={view === 'records'} onclick={() => go('records')}><span>▤</span>{deployment.terminology.record_plural}</button>{/if}{#if session.access_level === 0}<button class:active={view === 'admin'} onclick={() => go('admin')}><span>⚙</span>{deployment.terminology.administration_label}<small>Admin</small></button>{/if}</nav>
    {#if view === 'dashboard'}<DashboardView isAdmin={session.access_level === 0} revision={dashboardRevision} dashboardLabel={deployment.terminology.dashboard_label} recordSingular={deployment.terminology.record_singular} recordPlural={deployment.terminology.record_plural} />
    {:else if view === 'records'}<RecordsView accessLevel={session.access_level} revision={recordRevision} liveMessage={lastRecordEvent} recordSingular={deployment.terminology.record_singular} recordPlural={deployment.terminology.record_plural} />
    {:else if view === 'admin' && session.access_level === 0}{#key adminRevision}<AdminView {session} administrationLabel={deployment.terminology.administration_label} dashboardLabel={deployment.terminology.dashboard_label} />{/key}
    {:else}<AccountSecurity {session} {onSessionChanged} {onSessionEnded} />{/if}
  </main>
  {#if mobileOpen}<button class="sidebar-scrim" aria-label="Close menu" onclick={() => mobileOpen = false}></button>{/if}
</div>

{#if settingsOpen}<div class="overlay" role="presentation" onclick={(event) => { if (event.currentTarget === event.target) settingsOpen = false; }}><div class="dialog settings-dialog" role="dialog" aria-modal="true" aria-labelledby="display-settings-title"><div class="dialog-head"><div><p class="eyebrow">Personal preferences</p><h2 id="display-settings-title">Display and refresh</h2><p class="muted">These choices are stored only in this browser.</p></div><button class="icon-button" aria-label="Close" onclick={() => settingsOpen = false}>×</button></div><form class="form-stack" onsubmit={(event) => { event.preventDefault(); saveDisplaySettings(); }}><label>Theme<select bind:value={theme} onchange={() => setTheme(theme)}><option value="light">Light</option><option value="dark">Dark</option><option value="system">Use system setting</option></select></label><label class="checkbox setting-checkbox"><input type="checkbox" bind:checked={autoScale} onchange={applyScale} /> Automatically fit the interface to this screen</label><label>Interface size: {autoScale ? `${recommendedScale()}% recommended` : `${scale}%`}<input type="range" min="85" max="160" step="5" bind:value={scale} disabled={autoScale} oninput={applyScale} /></label><label>Automatic data refresh<select bind:value={autoRefresh}><option value={0}>Off — live updates only</option><option value={30}>Every 30 seconds</option><option value={60}>Every minute</option><option value={300}>Every 5 minutes</option><option value={900}>Every 15 minutes</option></select></label><div class="dialog-actions"><button class="button" type="button" onclick={() => settingsOpen = false}>Cancel</button><span class="spacer"></span><button class="button primary">Save preferences</button></div></form></div></div>{/if}
