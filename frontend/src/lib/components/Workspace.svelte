<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from '@lucide/svelte/icons/bell';
  import Download from '@lucide/svelte/icons/download';
  import FileText from '@lucide/svelte/icons/file-text';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Menu from '@lucide/svelte/icons/menu';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import MoonStar from '@lucide/svelte/icons/moon-star';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Settings from '@lucide/svelte/icons/settings';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import UserRound from '@lucide/svelte/icons/user-round';
  import X from '@lucide/svelte/icons/x';
  import type { DeploymentConfig, ModuleDefinition, MxNotification, UserPreferences } from '../api/domain';
  import type { Session } from '../api/types';
  import { exportReport, loadModules, loadNotificationSound, loadNotificationSoundInfo, loadNotifications, loadPreferences, markNotificationRead, savePreferences } from '../api/workspace';
  import { MxLiveClient, type LiveMessage } from '../live/client';
  import { appearanceScale, primaryForeground, validAccentColor } from '../util/appearance';
  import { decodeNotificationSound, playNotificationSound, unlockNotificationAudio } from '../util/notificationAudio';
  import { notificationPresentation } from '../util/notificationKinds';
  import AccountSecurity from './AccountSecurity.svelte';
  import AdminView from './AdminView.svelte';
  import CollaborationView from './CollaborationView.svelte';
  import DashboardView from './DashboardView.svelte';
  import GlobalSearch from './GlobalSearch.svelte';
  import ModuleIcon from './ModuleIcon.svelte';
  import NotificationCenter from './NotificationCenter.svelte';
  import NotificationKindIcon from './NotificationKindIcon.svelte';
  import RecordsView from './RecordsView.svelte';

  let { session, deployment, onSessionChanged, onSessionEnded, onDeploymentChanged }:
    { session: Session; deployment: DeploymentConfig; onSessionChanged: () => Promise<unknown>; onSessionEnded: (message: string) => void; onDeploymentChanged: () => Promise<void> } = $props();
  type View = 'dashboard' | 'records' | 'collaboration' | 'admin' | 'account';
  let view = $state<View>('dashboard');
  let recordRevision = $state(0); let dashboardRevision = $state(0); let adminRevision = $state(0);
  let live = $state(false); let mobileOpen = $state(false); let settingsOpen = $state(false);
  let notificationsOpen = $state(false); let notificationRevision = $state(0); let unreadNotifications = $state(0);
  let collaborationRevision = $state(0);
  let notificationPreferences = $state<UserPreferences['notifications']>(defaultNotificationPreferences());
  let modules = $state<ModuleDefinition[]>([]);
  let selectedModuleUid = $state('mx-default-records');
  let linkedRecordUid = $state('');
  let linkedChannelUid = $state('');
  let lastRecordEvent = $state<LiveMessage | null>(null);
  let lastCollaborationEvent = $state<LiveMessage | null>(null);
  let autoScale = $state(true); let scale = $state(100); let fontScale = $state(100); let autoRefresh = $state(0); let theme = $state(''); let accentColor = $state('');
  let density = $state(''); let contentWidth = $state(''); let reducedMotion = $state(false); let settingsBusy = $state(false); let settingsError = $state('');
  let notificationToasts = $state<MxNotification[]>([]);
  let customNotificationSound: AudioBuffer | null = null;
  const notificationToastTimers = new Map<string, number>();
  const accentPresets = ['#1d4ed8', '#047857', '#6d28d9', '#b45309', '#be123c', '#0e7490'];
  const selectedModule = $derived(modules.find((module) => module.uid === selectedModuleUid) || null);
  const selectedModulePermission = $derived(selectedModule?.permissions.find((permission) => permission.access_level === session.access_level) || null);
  const viewTitle = $derived(view === 'dashboard' ? deployment.terminology.dashboard_label : view === 'records' ? (selectedModule?.name || deployment.terminology.record_plural) : view === 'collaboration' ? 'Collaboration' : view === 'admin' ? deployment.terminology.administration_label : 'My account');

  onMount(() => {
    applyScale();
    applyPersonalAppearance();
    void loadAccountPreferences();
    void loadWorkspaceModules();
    const fromHash = hashView();
    view = fromHash || defaultView();
    if (!fromHash) history.replaceState(null, '', `#${view}`);
    const client = new MxLiveClient(handleLive, (connected) => {
      live = connected;
      if (connected) dashboardRevision += 1;
    });
    client.start();
    const handleHash = () => { const next = hashView(); if (next) { view = next; selectModuleFromHash(); mobileOpen = false; } };
    const handleResize = () => { if (autoScale) applyScale(); };
    const unlockAudio = () => void unlockNotificationAudio().catch(() => undefined);
    window.addEventListener('hashchange', handleHash);
    window.addEventListener('resize', handleResize);
    window.addEventListener('pointerdown', unlockAudio, { once: true });
    window.addEventListener('keydown', unlockAudio, { once: true });
    if (navigator.userActivation?.hasBeenActive) unlockAudio();
    return () => { client.stop(); live = false; window.removeEventListener('hashchange', handleHash); window.removeEventListener('resize', handleResize); window.removeEventListener('pointerdown', unlockAudio); window.removeEventListener('keydown', unlockAudio); for (const timer of notificationToastTimers.values()) window.clearTimeout(timer); resetPersonalAppearance(); };
  });

  async function loadWorkspaceModules() {
    try {
      const result = await loadModules();
      modules = result.modules.filter((module) => module.active);
      selectModuleFromHash();
      if (!modules.some((module) => module.uid === selectedModuleUid)) selectedModuleUid = modules[0]?.uid || 'mx-default-records';
    } catch {
      modules = [];
    }
  }

  function selectModuleFromHash() {
    const match = location.hash.match(/^#\/?module\/([^/?#]+)/);
    if (!match) {
      linkedRecordUid = '';
      const raw = location.hash.replace(/^#\/?/, '');
      linkedChannelUid = raw.split('?')[0] === 'collaboration' ? new URLSearchParams(raw.split('?')[1] || '').get('channel') || '' : '';
      return;
    }
    linkedChannelUid = '';
    const uid = decodeURIComponent(match[1]);
    if (!modules.length || modules.some((module) => module.uid === uid)) selectedModuleUid = uid;
    const query = location.hash.split('?')[1] || '';
    linkedRecordUid = new URLSearchParams(query).get('record') || '';
  }

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
    if (next === 'collaboration') return true;
    if (next === 'admin') return session.access_level === 0;
    return true;
  }
  function hashView(): View | null { if (/^#\/?module\//.test(location.hash)) return allowed('records') ? 'records' : null; const candidate = location.hash.replace(/^#\/?/, '').split('?')[0] as View; return ['dashboard', 'records', 'collaboration', 'admin', 'account'].includes(candidate) && allowed(candidate) ? candidate : null; }
  function handleLive(message: LiveMessage) {
    if (message.type === 'notification.created' || message.type.startsWith('notification.')) {
      notificationRevision += 1;
      if (message.type === 'notification.created') announceNotification(message);
      else void refreshUnreadCount();
      return;
    }
    if (message.type === 'preferences.updated') { void loadAccountPreferences(); return; }
    if (message.type.startsWith('message.') || message.type.startsWith('channel.') || message.type.startsWith('file.')) { lastCollaborationEvent = message; collaborationRevision += 1; notificationRevision += 1; return; }
    if (message.type.startsWith('module.')) { void loadWorkspaceModules(); recordRevision += 1; dashboardRevision += 1; return; }
    if (message.type === 'presence.changed') { dashboardRevision += 1; collaborationRevision += 1; return; }
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
  function inQuietHours(start: string | null, end: string | null) {
    if (!start || !end) return false;
    const now = new Date(); const current = now.getHours() * 60 + now.getMinutes();
    const parse = (value: string) => { const [hours, minutes] = value.split(':').map(Number); return hours * 60 + minutes; };
    const from = parse(start); const to = parse(end);
    return from <= to ? current >= from && current < to : current >= from || current < to;
  }
  function asNotification(message: LiveMessage): MxNotification | null {
    if (!message.payload || typeof message.payload !== 'object') return null;
    const payload = message.payload as Partial<MxNotification>;
    if (typeof payload.uid !== 'string' || typeof payload.title !== 'string') return null;
    return {
      uid: payload.uid, kind: String(payload.kind || 'notification'), title: payload.title,
      body: String(payload.body || ''), actor_uid: payload.actor_uid || null,
      actor_name: payload.actor_name || null, target_type: payload.target_type || null,
      target_uid: payload.target_uid || null, module_uid: payload.module_uid || null,
      data: payload.data || {}, created_at: Number(payload.created_at || Date.now()), read_at: null
    };
  }
  function alertImmediately() { const digest = notificationPreferences?.digest; return !digest || digest === 'immediate'; }
  function announceNotification(message: LiveMessage) {
    const item = asNotification(message);
    unreadNotifications += 1;
    if (!item || !alertImmediately() || inQuietHours(notificationPreferences.quiet_hours_start, notificationPreferences.quiet_hours_end)) return;
    notificationToasts = [item, ...notificationToasts.filter((toast) => toast.uid !== item.uid)].slice(0, 4);
    const existing = notificationToastTimers.get(item.uid); if (existing) window.clearTimeout(existing);
    notificationToastTimers.set(item.uid, window.setTimeout(() => dismissToast(item.uid), 9000));
    void deliverDeviceAlert(item);
  }
  async function deliverDeviceAlert(item: MxNotification) {
    // A user can have MX open in several tabs. A short randomized claim keeps
    // those tabs from playing the same sound and OS notification in chorus.
    await new Promise((resolve) => window.setTimeout(resolve, Math.floor(Math.random() * 80)));
    const key = `mx-notification-alert:${item.uid}`;
    const now = Date.now();
    try {
      const previous = Number(localStorage.getItem(key)?.split(':')[0] || 0);
      if (previous > now - 15_000) return;
      const claim = `${now}:${crypto.randomUUID()}`;
      localStorage.setItem(key, claim);
      window.setTimeout(() => { if (localStorage.getItem(key) === claim) localStorage.removeItem(key); }, 15_000);
    } catch { /* Storage can be unavailable in privacy modes; alert this tab. */ }
    if (notificationPreferences.sound_enabled) void playNotificationSound(notificationPreferences.sound_source === 'custom' ? customNotificationSound : null, notificationPreferences.sound_volume ?? 70).catch(() => undefined);
    showBrowserNotification(item);
  }
  function dismissToast(uid: string) {
    const timer = notificationToastTimers.get(uid); if (timer) window.clearTimeout(timer);
    notificationToastTimers.delete(uid); notificationToasts = notificationToasts.filter((item) => item.uid !== uid);
  }
  function openNotification(item: MxNotification) {
    dismissToast(item.uid);
    if (!item.read_at) { item.read_at = Date.now(); unreadNotifications = Math.max(0, unreadNotifications - 1); notificationRevision += 1; void markNotificationRead(item.uid); }
    if (item.target_type === 'record' && item.module_uid && item.target_uid) location.hash = `module/${encodeURIComponent(item.module_uid)}?record=${encodeURIComponent(item.target_uid)}`;
    else if (item.target_type === 'channel' && item.target_uid) location.hash = `collaboration?channel=${encodeURIComponent(item.target_uid)}`;
    else if (item.module_uid) location.hash = `module/${encodeURIComponent(item.module_uid)}`;
  }
  function showBrowserNotification(item: MxNotification) {
    const preferences = notificationPreferences;
    if (!preferences?.browser_enabled || !('Notification' in window) || Notification.permission !== 'granted') return;
    const notification = new Notification(item.title || deployment.branding.display_name, { body: item.body || 'You have a new MX notification.', icon: deployment.branding.logo_url || '/images/system-icon.png', tag: item.uid });
    notification.onclick = () => { window.focus(); openNotification(item); notification.close(); };
  }
  async function refreshUnreadCount() {
    try { unreadNotifications = (await loadNotifications(true)).unread; }
    catch { /* Preserve the last known count while connectivity recovers. */ }
  }
  function go(next: View, replace = false) {
    if (!allowed(next)) return;
    view = next; mobileOpen = false; linkedChannelUid = '';
    const url = `#${next}`;
    if (location.hash !== url) history[replace ? 'replaceState' : 'pushState'](null, '', url);
  }
  function goModule(uid: string, replace = false) {
    const module = modules.find((item) => item.uid === uid);
    if (!module) return;
    selectedModuleUid = uid; linkedRecordUid = ''; view = 'records'; mobileOpen = false;
    const url = `#module/${encodeURIComponent(uid)}`;
    if (location.hash !== url) history[replace ? 'replaceState' : 'pushState'](null, '', url);
  }
  function refreshCurrent() {
    if (view === 'records') recordRevision += 1;
    else if (view === 'dashboard') dashboardRevision += 1;
    else if (view === 'admin') adminRevision += 1;
  }
  function initials(name: string) { return name.split(/\s+/).map((part) => part[0]).join('').slice(0, 2).toUpperCase(); }
  function moduleGroup(module: ModuleDefinition) { return typeof module.config.navigation_group === 'string' ? module.config.navigation_group.trim() : ''; }
  function setTheme(value: string) { theme = value; document.documentElement.dataset.theme = value || deployment.appearance.default_theme; }
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
    const font = Math.min(150, Math.max(85, Number(fontScale) || 100));
    // Typography and interface density are separate preferences. Previously
    // both values were multiplied into the root font size, making the two
    // controls behave identically.
    document.documentElement.style.removeProperty('font-size');
    const { interfaceRatio, fontRatio, applicationFontRatio, viewportPercent } = appearanceScale(value, font);
    document.documentElement.style.setProperty('--interface-scale', String(interfaceRatio));
    document.documentElement.style.setProperty('--font-scale', String(fontRatio));
    // Compensate text for interface zoom so the two preferences remain independent.
    document.documentElement.style.setProperty('--application-font-scale', String(applicationFontRatio));
    document.documentElement.style.setProperty('--interface-viewport-width', `${viewportPercent}%`);
    document.documentElement.style.setProperty('--interface-viewport-height', `${viewportPercent}vh`);
  }
  function applyPersonalAppearance() {
    setTheme(theme);
    const accent = validAccentColor(accentColor) ? accentColor : deployment.appearance.primary_color;
    document.documentElement.style.setProperty('--primary', accent);
    document.documentElement.style.setProperty('--on-primary', primaryForeground(accent));
    document.documentElement.dataset.density = density || deployment.appearance.density;
    const width = contentWidth || deployment.appearance.content_width;
    document.documentElement.style.setProperty('--content-max', ({ standard: '76rem', wide: '92rem', full: '120rem' } as Record<string, string>)[width] || '92rem');
    document.documentElement.dataset.reducedMotion = reducedMotion ? 'true' : 'false';
    applyScale();
  }
  function resetPersonalAppearance() {
    for (const property of ['--interface-scale', '--font-scale', '--application-font-scale', '--interface-viewport-width', '--interface-viewport-height']) document.documentElement.style.removeProperty(property);
    document.documentElement.dataset.theme = deployment.appearance.default_theme;
    document.documentElement.dataset.density = deployment.appearance.density;
    document.documentElement.dataset.reducedMotion = 'false';
    document.documentElement.style.setProperty('--primary', deployment.appearance.primary_color);
    document.documentElement.style.setProperty('--on-primary', primaryForeground(deployment.appearance.primary_color));
    document.documentElement.style.setProperty('--content-max', ({ standard: '76rem', wide: '92rem', full: '120rem' } as Record<string, string>)[deployment.appearance.content_width] || '92rem');
  }
  function defaultNotificationPreferences(): UserPreferences['notifications'] {
    return { browser_enabled: false, sound_enabled: true, sound_source: 'default', sound_volume: 70, messages: null, mentions: null, channel_activity: null, record_created: null, record_updated: null, record_assigned: null, attachment_received: null, workflow_changes: null, task_activity: null, digest: null, quiet_hours_start: null, quiet_hours_end: null };
  }
  async function loadAccountPreferences() {
    try {
      const [response, inbox] = await Promise.all([loadPreferences(), loadNotifications(true)]);
      const preferences = response.preferences;
      theme = preferences.theme || '';
      accentColor = preferences.accent_color || '';
      autoScale = preferences.auto_scale ?? true;
      scale = preferences.ui_scale_percent ?? 100;
      fontScale = preferences.font_scale_percent ?? 100;
      autoRefresh = preferences.auto_refresh_seconds ?? 0;
      density = preferences.density || '';
      contentWidth = preferences.content_width || '';
      reducedMotion = preferences.reduced_motion ?? false;
      notificationPreferences = { ...preferences.notifications };
      unreadNotifications = inbox.unread;
      await prepareNotificationSound();
      applyPersonalAppearance();
    } catch (reason) {
      settingsError = reason instanceof Error ? reason.message : 'Could not load account preferences.';
    }
  }
  async function prepareNotificationSound() {
    customNotificationSound = null;
    if (!notificationPreferences?.sound_enabled || notificationPreferences.sound_source !== 'custom') return;
    try { const info = await loadNotificationSoundInfo(); if (info.exists) customNotificationSound = await decodeNotificationSound(await loadNotificationSound(info.updated_at)); }
    catch { customNotificationSound = null; }
  }
  async function saveDisplaySettings() {
    scale = Math.min(160, Math.max(85, Number(scale) || 100));
    fontScale = Math.min(150, Math.max(85, Number(fontScale) || 100));
    autoRefresh = Math.max(0, Number(autoRefresh) || 0);
    settingsBusy = true; settingsError = '';
    try {
      const current = await loadPreferences();
      await savePreferences({
        ...current.preferences,
        theme: (theme || null) as UserPreferences['theme'], auto_scale: autoScale,
        accent_color: validAccentColor(accentColor) ? accentColor : null,
        ui_scale_percent: scale, font_scale_percent: fontScale,
        density: (density || null) as UserPreferences['density'],
        content_width: (contentWidth || null) as UserPreferences['content_width'],
        reduced_motion: reducedMotion, auto_refresh_seconds: autoRefresh,
        notifications: current.preferences.notifications || defaultNotificationPreferences()
      });
      notificationPreferences = { ...current.preferences.notifications };
      applyPersonalAppearance(); settingsOpen = false;
    } catch (reason) {
      settingsError = reason instanceof Error ? reason.message : 'Could not save account preferences.';
    } finally { settingsBusy = false; }
  }
</script>

<div class="application">
  <aside class:open={mobileOpen} class="sidebar">
    <div class="sidebar-brand">{#if deployment.branding.logo_url}<img src={deployment.branding.logo_url} alt="" />{:else}<span>{initials(deployment.branding.display_name)}</span>{/if}<div><strong>{deployment.branding.display_name}</strong><small>{deployment.branding.subtitle}</small></div></div>
    <nav class="main-nav" aria-label="Main navigation">
      {#if deployment.navigation.show_dashboard}<button class:active={view === 'dashboard'} onclick={() => go('dashboard')}><span><LayoutDashboard size={17} /></span>{deployment.terminology.dashboard_label}</button>{/if}
      {#if modules.length}{#each modules as module, index}{#if moduleGroup(module) && (index === 0 || moduleGroup(modules[index - 1]) !== moduleGroup(module))}<span class="nav-group-label">{moduleGroup(module)}</span>{/if}<button class:active={view === 'records' && selectedModuleUid === module.uid} onclick={() => goModule(module.uid)}><span style={`color:${module.color}`}><ModuleIcon name={module.icon} size={17} /></span>{module.name}</button>{/each}{:else if deployment.navigation.show_records}<button class:active={view === 'records'} onclick={() => go('records')}><span><FileText size={17} /></span>{deployment.terminology.record_plural}</button>{/if}<button class:active={view === 'collaboration'} onclick={() => go('collaboration')}><span><MessagesSquare size={17} /></span>Collaboration</button>
      {#if session.access_level === 0}<button class:active={view === 'admin'} onclick={() => go('admin')}><span><ShieldCheck size={17} /></span>{deployment.terminology.administration_label}</button>{/if}
    </nav>
    {#if deployment.navigation.show_quick_actions}<section class="quick-actions"><span>Quick actions</span>{#if deployment.navigation.show_records}<button onclick={() => goModule(selectedModule?.uid || modules[0]?.uid || 'mx-default-records')}><i><FileText size={15} /></i>Open {(selectedModule?.singular_name || deployment.terminology.record_singular).toLowerCase()} workspace</button>{/if}<button onclick={() => exportReport('records', undefined, '', '', selectedModule?.uid || 'mx-default-records')}><i><Download size={15} /></i>Export detailed {(selectedModule?.name || deployment.terminology.record_plural).toLowerCase()}</button></section>{/if}
    <div class="sidebar-foot"><div class="live-state"><i class:online={live}></i>{live ? 'Live sync connected' : 'Reconnecting live sync'}</div><button class:active={view === 'account'} class="profile-button" onclick={() => go('account')}><span>{initials(session.name)}</span><div><strong>{session.name}</strong><small>{session.access_name} · My account</small></div></button></div>
  </aside>
  <main class="main-stage">
    <header class="mobile-header"><button class="icon-button" aria-label="Open navigation" onclick={() => mobileOpen = !mobileOpen}><Menu size={20} /></button><strong>{viewTitle}</strong><button class="button small icon-label" onclick={refreshCurrent}><RefreshCw size={15} />Refresh</button></header>
    <header class="workspace-topbar"><div class="topbar-context"><button class="menu-button" aria-label="Open navigation menu" aria-expanded={mobileOpen} onclick={() => mobileOpen = !mobileOpen}><Menu size={18} /></button><div class="workspace-identity"><small>{deployment.branding.display_name}</small><strong>{viewTitle}</strong></div></div><div class="desktop-actions"><GlobalSearch /><span class:online={live} class="topbar-live">{live ? 'Live' : 'Reconnecting'}</span><button class="topbar-tool" type="button" title="Refresh current workspace" aria-label="Refresh current workspace" onclick={refreshCurrent}><RefreshCw size={17} /></button><div class="notification-anchor"><button class="topbar-tool notification-button" type="button" title="Notifications" aria-label={`Notifications${unreadNotifications ? `, ${unreadNotifications} unread` : ''}`} aria-expanded={notificationsOpen} onclick={() => notificationsOpen = !notificationsOpen}><Bell size={17} />{#if unreadNotifications}<span>{unreadNotifications > 99 ? '99+' : unreadNotifications}</span>{/if}</button>{#if notificationsOpen}<NotificationCenter revision={notificationRevision} onUnread={(count) => unreadNotifications = count} onClose={() => notificationsOpen = false} />{/if}</div><button class="topbar-tool" type="button" title="Personal settings" aria-label="Personal settings" onclick={() => settingsOpen = true}><Settings size={17} /></button><button class="topbar-tool" type="button" title="Change theme" aria-label="Change theme" onclick={toggleTheme}><MoonStar size={17} /></button><button class="topbar-tool" type="button" title="My account" aria-label="My account" onclick={() => go('account')}><UserRound size={17} /></button><button class="topbar-tool sign-out-button" type="button" title="Sign out" aria-label="Sign out" onclick={() => onSessionEnded('Signed out.')}><LogOut size={17} /></button></div></header>
    <nav class="workspace-tabs" aria-label="Workspace tabs">{#if deployment.navigation.show_dashboard}<button class:active={view === 'dashboard'} onclick={() => go('dashboard')}><span><LayoutDashboard size={15} /></span>{deployment.terminology.dashboard_label}</button>{/if}{#if modules.length}{#each modules as module}<button class:active={view === 'records' && selectedModuleUid === module.uid} onclick={() => goModule(module.uid)}><span style={`color:${module.color}`}><ModuleIcon name={module.icon} size={15} /></span>{module.name}</button>{/each}{:else if deployment.navigation.show_records}<button class:active={view === 'records'} onclick={() => go('records')}><span><FileText size={15} /></span>{deployment.terminology.record_plural}</button>{/if}<button class:active={view === 'collaboration'} onclick={() => go('collaboration')}><span><MessagesSquare size={15} /></span>Collaboration</button>{#if session.access_level === 0}<button class:active={view === 'admin'} onclick={() => go('admin')}><span><ShieldCheck size={15} /></span>{deployment.terminology.administration_label}<small>Admin</small></button>{/if}</nav>
    {#if view === 'dashboard'}<DashboardView isAdmin={session.access_level === 0} revision={dashboardRevision} dashboardLabel={deployment.terminology.dashboard_label} recordSingular={deployment.terminology.record_singular} recordPlural={deployment.terminology.record_plural} />
    {:else if view === 'records'}{#key selectedModuleUid}<RecordsView accessLevel={session.access_level} moduleUid={selectedModule?.uid || ''} modulePermission={selectedModulePermission} openRecordUid={linkedRecordUid} revision={recordRevision} liveMessage={lastRecordEvent} recordSingular={selectedModule?.singular_name || deployment.terminology.record_singular} recordPlural={selectedModule?.name || deployment.terminology.record_plural} />{/key}
    {:else if view === 'collaboration'}<CollaborationView {session} openChannelUid={linkedChannelUid} revision={collaborationRevision} connected={live} liveEvent={lastCollaborationEvent} />
    {:else if view === 'admin' && session.access_level === 0}{#key adminRevision}<AdminView {session} administrationLabel={deployment.terminology.administration_label} dashboardLabel={deployment.terminology.dashboard_label} />{/key}
    {:else}<AccountSecurity {session} {onSessionChanged} {onSessionEnded} />{/if}
  </main>
  {#if mobileOpen}<button class="sidebar-scrim" aria-label="Close menu" onclick={() => mobileOpen = false}></button>{/if}
</div>

{#if notificationToasts.length}
  <section class="notification-toast-stack" aria-label="New notifications" aria-live="polite">
    {#each notificationToasts as item (item.uid)}
      {@const presentation = notificationPresentation(item)}
      <article class="notification-toast" data-category={presentation.category}>
        <button class="notification-toast-open" type="button" onclick={() => openNotification(item)}>
          <span class="notification-toast-icon"><NotificationKindIcon category={presentation.category} size={18} /></span>
          <span><small>{presentation.label}{item.actor_name ? ` · ${item.actor_name}` : ''}</small><strong>{item.title}</strong>{#if item.body}<em>{item.body}</em>{/if}</span>
        </button>
        <button class="notification-toast-close" type="button" aria-label={`Dismiss ${item.title}`} onclick={() => dismissToast(item.uid)}><X size={16} /></button>
      </article>
    {/each}
  </section>
{/if}

{#if settingsOpen}<div class="overlay" role="presentation" onclick={(event) => { if (event.currentTarget === event.target) settingsOpen = false; }}><div class="dialog settings-dialog" role="dialog" aria-modal="true" aria-labelledby="display-settings-title"><div class="dialog-head"><div><p class="eyebrow">Personal preferences</p><h2 id="display-settings-title">Display and refresh</h2><p class="muted">Saved to your MX account and applied over administrator defaults.</p></div><button class="icon-button" aria-label="Close" onclick={() => settingsOpen = false}><X size={19} /></button></div>{#if settingsError}<div class="notice error">{settingsError}</div>{/if}<form class="form-stack" onsubmit={(event) => { event.preventDefault(); void saveDisplaySettings(); }}><label>Theme<select bind:value={theme} onchange={() => setTheme(theme)}><option value="">Use administrator default</option><option value="light">Light</option><option value="dark">Dark</option><option value="system">Use system setting</option></select></label><fieldset class="personal-accent"><legend>Accent color</legend><div class="accent-swatches"><button class:active={!accentColor} type="button" style={`--swatch:${deployment.appearance.primary_color}`} onclick={() => { accentColor = ''; applyPersonalAppearance(); }} title="Use administrator default" aria-label="Use administrator default accent"><i></i></button>{#each accentPresets as color}<button class:active={accentColor === color} type="button" style={`--swatch:${color}`} onclick={() => { accentColor = color; applyPersonalAppearance(); }} title={color} aria-label={`Use accent ${color}`}><i></i></button>{/each}<label class="accent-custom" title="Choose a custom accent"><input type="color" value={accentColor || deployment.appearance.primary_color} oninput={(event) => { accentColor = event.currentTarget.value; applyPersonalAppearance(); }} /><span>Custom</span></label></div><small>{accentColor ? `Personal override · ${accentColor}` : 'Using the administrator default'}</small></fieldset><label>Density<select bind:value={density} onchange={applyPersonalAppearance}><option value="">Use administrator default</option><option value="compact">Compact</option><option value="normal">Normal</option><option value="comfortable">Comfortable</option></select></label><label>Content width<select bind:value={contentWidth} onchange={applyPersonalAppearance}><option value="">Use administrator default</option><option value="standard">Standard</option><option value="wide">Wide</option><option value="full">Full width</option></select></label><label class="checkbox setting-checkbox"><input type="checkbox" bind:checked={autoScale} onchange={applyScale} /> Automatically fit the interface to this screen</label><label>Interface size: {autoScale ? `${recommendedScale()}% recommended` : `${scale}%`}<input type="range" min="85" max="160" step="5" bind:value={scale} disabled={autoScale} oninput={applyScale} /></label><label>Font size: {fontScale}%<input type="range" min="85" max="150" step="5" bind:value={fontScale} oninput={applyScale} /></label><label class="checkbox setting-checkbox"><input type="checkbox" bind:checked={reducedMotion} onchange={applyPersonalAppearance} /> Reduce animation and motion</label><label>Automatic data refresh<select bind:value={autoRefresh}><option value={0}>Off — live updates only</option><option value={30}>Every 30 seconds</option><option value={60}>Every minute</option><option value={300}>Every 5 minutes</option><option value={900}>Every 15 minutes</option></select></label><div class="dialog-actions"><button class="button" type="button" onclick={() => settingsOpen = false}>Cancel</button><span class="spacer"></span><button class="button primary" disabled={settingsBusy}>{settingsBusy ? 'Saving…' : 'Save preferences'}</button></div></form></div></div>{/if}
