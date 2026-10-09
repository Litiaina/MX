<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import Phone from '@lucide/svelte/icons/phone';
  import PhoneIncoming from '@lucide/svelte/icons/phone-incoming';
  import PhoneOff from '@lucide/svelte/icons/phone-off';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import Video from '@lucide/svelte/icons/video';
  import X from '@lucide/svelte/icons/x';
  import type { ActiveCallSummary, CallMode, CallParticipant, CollaborationChannel, DeploymentConfig, ModuleDefinition, MxNotification, UserPreferences } from '../api/domain';
  import type { Session } from '../api/types';
  import { exportReport, listActiveCalls, loadModules, loadNotificationSound, loadNotificationSoundInfo, loadNotifications, loadPreferences, markNotificationRead, savePreferences } from '../api/workspace';
  import { MxLiveClient, type LiveMessage } from '../live/client';
  import { callDisplayName, callFromStartedPayload, mergeCallParticipant, removeCallParticipant } from '../call/activity';
  import { callAlertStrategy, startCallAlertCadence } from '../call/alerts';
  import { appearanceScale, primaryForeground, validAccentColor } from '../util/appearance';
  import { decodeNotificationSound, playNotificationSound, unlockNotificationAudio } from '../util/notificationAudio';
  import { notificationPresentation } from '../util/notificationKinds';
  import { recordNotificationIntent } from '../util/interactions';
  import { requestConfirmation } from '../confirmation';
  import AccountSecurity from './AccountSecurity.svelte';
  import AdminView from './AdminView.svelte';
  import CollaborationView from './CollaborationView.svelte';
  import DriveView from './DriveView.svelte';
  import DriveUploadPanel from './DriveUploadPanel.svelte';
  import { createDriveUploads } from '../drive/uploads.svelte';
  import CollaborationCall from './CollaborationCall.svelte';
  import DashboardView from './DashboardView.svelte';
  import DisplaySettingsDialog from './DisplaySettingsDialog.svelte';
  import NotificationKindIcon from './NotificationKindIcon.svelte';
  import RecordsView from './RecordsView.svelte';
  import WorkspaceSidebar from './WorkspaceSidebar.svelte';
  import WorkspaceTopbar from './WorkspaceTopbar.svelte';

  let { session, deployment, onSessionChanged, onSessionEnded, onDeploymentChanged }:
    { session: Session; deployment: DeploymentConfig; onSessionChanged: () => Promise<unknown>; onSessionEnded: (message: string) => void; onDeploymentChanged: () => Promise<void> } = $props();
  type View = 'dashboard' | 'records' | 'collaboration' | 'drive' | 'admin' | 'account';
  const driveUploads = untrack(() => createDriveUploads(session.uid));
  onDestroy(() => driveUploads.dispose());
  onMount(() => {
    const warnUploads = (event: BeforeUnloadEvent) => { if (driveUploads.active()) { event.preventDefault(); event.returnValue = ''; } };
    window.addEventListener('beforeunload', warnUploads);
    return () => window.removeEventListener('beforeunload', warnUploads);
  });
  let view = $state<View>('dashboard');
  let recordRevision = $state(0); let dashboardRevision = $state(0); let adminRevision = $state(0);
  let live = $state(false); let mobileOpen = $state(false); let settingsOpen = $state(false);
  let notificationRevision = $state(0); let unreadNotifications = $state(0);
  let collaborationRevision = $state(0);
  let activeCollaborationChannelUid = $state('');
  let activeCollaborationNewestVisible = $state(false);
  let notificationPreferences = $state<UserPreferences['notifications']>(defaultNotificationPreferences());
  let notificationPreferencesReady = false;
  let modules = $state<ModuleDefinition[]>([]);
  let selectedModuleUid = $state('mx-default-records');
  let linkedRecordUid = $state('');
  let linkedRecordFocusAttachments = $state(false);
  let linkedRecordRequestRevision = $state(0);
  let linkedChannelUid = $state('');
  let lastRecordEvent = $state<LiveMessage | null>(null);
  let lastCollaborationEvent = $state<LiveMessage | null>(null);
  let callEvents = $state<LiveMessage[]>([]);
  let activeCall = $state<{ channelUid: string; channelName: string; channelKind: string; mode: CallMode; nonce: number } | null>(null);
  let callSessionNotice = $state('');
  let callSessionNoticeTimer: number | undefined;
  let ongoingCalls = $state<ActiveCallSummary[]>([]);
  let dismissedCallUids = $state<string[]>([]);
  let autoScale = $state(true); let scale = $state(100); let fontScale = $state(16); let autoRefresh = $state(0); let theme = $state(''); let accentColor = $state('');
  let density = $state(''); let contentWidth = $state(''); let reducedMotion = $state(false); let settingsBusy = $state(false); let settingsError = $state('');
  let notificationToasts = $state<MxNotification[]>([]);
  let customNotificationSound: AudioBuffer | null = null;
  const notificationToastTimers = new Map<string, number>();
  const alertedCallKeys = new Set<string>();
  const callBrowserNotifications = new Map<string, Notification>();
  let ringingDirectCallKey = '';
  let stopDirectCallRingtone: (() => void) | null = null;
  const accentPresets = ['#1d4ed8', '#047857', '#6d28d9', '#b45309', '#be123c', '#0e7490'];
  const selectedModule = $derived(modules.find((module) => module.uid === selectedModuleUid) || null);
  const selectedModulePermission = $derived(selectedModule?.effective_permission || selectedModule?.permissions.find((permission) => permission.access_level === session.access_level) || null);
  const viewTitle = $derived(view === 'dashboard' ? deployment.terminology.dashboard_label : view === 'records' ? (selectedModule?.name || deployment.terminology.record_plural) : view === 'collaboration' ? 'Collaboration' : view === 'drive' ? 'MX Drive' : view === 'admin' ? deployment.terminology.administration_label : 'My account');
  const incomingCall = $derived(ongoingCalls
    .filter((call) => call.channel_uid !== activeCall?.channelUid && !call.participants.some((participant) => participant.user_uid === session.uid) && !dismissedCallUids.includes(call.channel_uid))
    .sort((left, right) => right.started_at - left.started_at)[0] || null);

  onMount(() => {
    applyScale();
    applyPersonalAppearance();
    void loadAccountPreferences().finally(() => { notificationPreferencesReady = true; void refreshOngoingCalls(); });
    void loadWorkspaceModules();
    const fromHash = hashView();
    view = fromHash || defaultView();
    if (!fromHash) history.replaceState(null, '', `#${view}`);
    const client = new MxLiveClient(handleLive, (connected) => {
      live = connected;
      if (connected) resynchronize();
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
    const callRefreshTimer = window.setInterval(() => void refreshOngoingCalls(), 15_000);
    return () => { client.stop(); live = false; window.clearInterval(callRefreshTimer); window.clearTimeout(callSessionNoticeTimer); window.removeEventListener('hashchange', handleHash); window.removeEventListener('resize', handleResize); window.removeEventListener('pointerdown', unlockAudio); window.removeEventListener('keydown', unlockAudio); for (const timer of notificationToastTimers.values()) window.clearTimeout(timer); stopIncomingCallAlert(); for (const notification of callBrowserNotifications.values()) notification.close(); callBrowserNotifications.clear(); resetPersonalAppearance(); };
  });

  async function loadWorkspaceModules() {
    try {
      const result = await loadModules();
      modules = result.modules.filter((module) => module.active);
      selectModuleFromHash();
      if (!modules.some((module) => module.uid === selectedModuleUid)) selectedModuleUid = modules[0]?.uid || 'mx-default-records';
    } catch {
      // Keep the last acknowledged navigation through a transient outage.
      // Every module/data request still enforces current server permissions.
    }
  }

  function selectModuleFromHash() {
    const match = location.hash.match(/^#\/?module\/([^/?#]+)/);
    if (!match) {
      linkedRecordUid = '';
      linkedRecordFocusAttachments = false;
      const raw = location.hash.replace(/^#\/?/, '');
      linkedChannelUid = raw.split('?')[0] === 'collaboration' ? new URLSearchParams(raw.split('?')[1] || '').get('channel') || '' : '';
      return;
    }
    linkedChannelUid = '';
    const uid = decodeURIComponent(match[1]);
    if (!modules.length || modules.some((module) => module.uid === uid)) selectedModuleUid = uid;
    const query = location.hash.split('?')[1] || '';
    const parameters = new URLSearchParams(query);
    linkedRecordUid = parameters.get('record') || '';
    linkedRecordFocusAttachments = parameters.get('attachments') === '1';
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
  function hashView(): View | null { if (/^#\/?module\//.test(location.hash)) return allowed('records') ? 'records' : null; const candidate = location.hash.replace(/^#\/?/, '').split('?')[0] as View; return ['dashboard', 'records', 'collaboration', 'drive', 'admin', 'account'].includes(candidate) && allowed(candidate) ? candidate : null; }
  function handleLive(message: LiveMessage) {
    if (message.type === 'notification.created' || message.type.startsWith('notification.')) {
      notificationRevision += 1;
      if (message.type === 'notification.created') announceNotification(message);
      else void refreshUnreadCount();
      return;
    }
    if (message.type === 'preferences.updated') { void loadAccountPreferences(); return; }
    if (message.type === 'profile.updated') { lastCollaborationEvent = message; collaborationRevision += 1; if (message.actor_uid === session.uid) void onSessionChanged(); return; }
    if (message.type.startsWith('call.')) {
      callEvents = [...callEvents.slice(-2047), message];
      applyGlobalCallEvent(message);
      lastCollaborationEvent = message;
      collaborationRevision += 1;
      return;
    }
    if (message.type.startsWith('message.') || message.type.startsWith('channel.') || message.type.startsWith('file.')) { lastCollaborationEvent = message; collaborationRevision += 1; notificationRevision += 1; return; }
    if (message.type.startsWith('module.')) { void loadWorkspaceModules(); recordRevision += 1; dashboardRevision += 1; return; }
    if (message.type === 'presence.changed') { dashboardRevision += 1; collaborationRevision += 1; return; }
    if (message.type === 'sync.required') {
      resynchronize();
      return;
    }
    if (message.type.startsWith('record.') || message.type.startsWith('attachment.') || message.type.startsWith('schema.')) { recordRevision += 1; lastRecordEvent = message; }
    if (message.type.startsWith('record.') || message.type.startsWith('attachment.') || message.type === 'dashboard.updated') dashboardRevision += 1;
    if (message.type === 'deployment.updated') void onDeploymentChanged();
  }
  function resynchronize() {
    recordRevision += 1; dashboardRevision += 1; adminRevision += 1;
    notificationRevision += 1; collaborationRevision += 1;
    lastRecordEvent = { type: 'sync.required' };
    lastCollaborationEvent = { type: 'sync.required' };
    void loadWorkspaceModules(); void refreshUnreadCount(); void refreshOngoingCalls();
    void loadAccountPreferences(); void onDeploymentChanged();
  }
  async function startCollaborationCall(channel: CollaborationChannel, mode: CallMode) {
    if (activeCall?.channelUid === channel.uid) return;
    if (activeCall && !await requestConfirmation({
      title: `Leave the call in ${activeCall.channelName}?`,
      description: `MX can connect this browser to one call at a time. Starting a call in ${channel.name} will leave the current call.`,
      confirmLabel: 'Switch call',
      tone: 'primary'
    })) return;
    activeCall = { channelUid: channel.uid, channelName: channel.name, channelKind: channel.kind, mode, nonce: Date.now() };
    stopIncomingCallAlert(channel.uid);
    dismissedCallUids = dismissedCallUids.filter((uid) => uid !== channel.uid);
  }
  async function refreshOngoingCalls() {
    try {
      ongoingCalls = (await listActiveCalls()).calls;
      if (ringingDirectCallKey && !ongoingCalls.some((call) => callAlertKey(call) === ringingDirectCallKey)) stopIncomingCallAlert();
      const freshIncoming = newestIncomingCall(ongoingCalls);
      if (freshIncoming) void deliverIncomingCallAlert(freshIncoming, freshIncoming.participants[0]?.user_name || freshIncoming.channel_name);
    }
    catch { /* Live call events continue to maintain state while MX retries. */ }
  }
  function applyGlobalCallEvent(message: LiveMessage) {
    if (!message.payload || typeof message.payload !== 'object') return;
    const payload = message.payload as Record<string, unknown>;
    const channelUid = typeof payload.channel_uid === 'string' ? payload.channel_uid : '';
    if (!channelUid) return;
    if (message.type === 'call.started') {
      const summary = callFromStartedPayload(payload, message.at || Date.now());
      if (!summary) { void refreshOngoingCalls(); return; }
      const participant = summary.participants[0];
      ongoingCalls = [summary, ...ongoingCalls.filter((call) => call.channel_uid !== channelUid)];
      dismissedCallUids = dismissedCallUids.filter((uid) => uid !== channelUid);
      if (participant.user_uid !== session.uid) void deliverIncomingCallAlert(summary, participant.user_name);
      return;
    }
    if (message.type === 'call.participant.joined' || message.type === 'call.participant.updated') {
      if (!payload.participant || typeof payload.participant !== 'object') { void refreshOngoingCalls(); return; }
      const participant = payload.participant as unknown as CallParticipant;
      const existing = ongoingCalls.find((call) => call.channel_uid === channelUid);
      if (!existing) { void refreshOngoingCalls(); return; }
      ongoingCalls = mergeCallParticipant(ongoingCalls, channelUid, participant);
      if (participant.user_uid === session.uid) stopIncomingCallAlert(channelUid);
      return;
    }
    if (message.type === 'call.participant.left') {
      const userUid = String(payload.user_uid || '');
      ongoingCalls = removeCallParticipant(ongoingCalls, channelUid, userUid, String(payload.session_uid || ''));
      if (!ongoingCalls.some((call) => call.channel_uid === channelUid)) stopIncomingCallAlert(channelUid);
      return;
    }
    if (message.type === 'call.ended') {
      ongoingCalls = ongoingCalls.filter((call) => call.channel_uid !== channelUid);
      dismissedCallUids = dismissedCallUids.filter((uid) => uid !== channelUid);
      stopIncomingCallAlert(channelUid);
    }
  }
  async function answerOngoingCall(call: ActiveCallSummary, mode: CallMode) {
    if (activeCall && activeCall.channelUid !== call.channel_uid && !await requestConfirmation({
      title: `Leave the call in ${activeCall.channelName}?`,
      description: `Joining ${call.channel_name} will disconnect this browser from the current call.`,
      confirmLabel: 'Switch call',
      tone: 'primary'
    })) return;
    stopIncomingCallAlert(call.channel_uid);
    activeCall = { channelUid: call.channel_uid, channelName: callDisplayName(call, session.uid), channelKind: call.channel_kind, mode, nonce: Date.now() };
    dismissedCallUids = dismissedCallUids.filter((uid) => uid !== call.channel_uid);
  }
  function dismissIncomingCall(channelUid: string) {
    stopIncomingCallAlert(channelUid);
    if (!dismissedCallUids.includes(channelUid)) dismissedCallUids = [...dismissedCallUids, channelUid];
  }
  function endLocalCall(reason?: 'replaced') {
    activeCall = null;
    if (reason !== 'replaced') return;
    callSessionNotice = 'This call continued in another MX tab signed in to your account.';
    window.clearTimeout(callSessionNoticeTimer);
    callSessionNoticeTimer = window.setTimeout(() => callSessionNotice = '', 7000);
  }
  function callAlertKey(call: ActiveCallSummary) { return `${call.channel_uid}:${call.started_at}`; }
  function newestIncomingCall(calls: ActiveCallSummary[]) {
    return calls
      .filter((call) => call.channel_uid !== activeCall?.channelUid && !call.participants.some((participant) => participant.user_uid === session.uid) && !dismissedCallUids.includes(call.channel_uid))
      .sort((left, right) => right.started_at - left.started_at)[0] || null;
  }
  function stopIncomingCallAlert(channelUid = '') {
    if (!channelUid || ringingDirectCallKey.startsWith(`${channelUid}:`)) {
      stopDirectCallRingtone?.();
      stopDirectCallRingtone = null;
      ringingDirectCallKey = '';
    }
    for (const [key, notification] of callBrowserNotifications) {
      if (channelUid && !key.startsWith(`${channelUid}:`)) continue;
      notification.close();
      callBrowserNotifications.delete(key);
    }
  }
  async function deliverIncomingCallAlert(call: ActiveCallSummary, callerName: string) {
    if (!notificationPreferencesReady) return;
    const alertKey = callAlertKey(call);
    if (alertedCallKeys.has(alertKey)) return;
    alertedCallKeys.add(alertKey);
    if (alertedCallKeys.size > 256) alertedCallKeys.delete(alertedCallKeys.values().next().value!);
    const strategy = callAlertStrategy(call, callerName);
    const elapsedMs = Math.max(0, Date.now() - call.started_at);
    if (elapsedMs >= strategy.audibleWindowMs) return;

    // Prefer the visible tab when one account is signed in more than once.
    const claimDelay = document.visibilityState === 'visible' && document.hasFocus()
      ? Math.floor(Math.random() * 40)
      : 120 + Math.floor(Math.random() * 100);
    await new Promise((resolve) => window.setTimeout(resolve, claimDelay));
    const current = ongoingCalls.find((item) => callAlertKey(item) === alertKey);
    if (!current || current.participants.some((participant) => participant.user_uid === session.uid) || dismissedCallUids.includes(call.channel_uid)) return;
    const claimKey = `mx-call-alert:${alertKey}`;
    try {
      if (localStorage.getItem(claimKey)) return;
      const claim = `${Date.now()}:${crypto.randomUUID()}`;
      localStorage.setItem(claimKey, claim);
      window.setTimeout(() => { if (localStorage.getItem(claimKey) === claim) localStorage.removeItem(claimKey); }, strategy.audibleWindowMs);
    } catch { /* Alert this tab when storage is unavailable. */ }
    if (notificationPreferences.sound_enabled && !inQuietHours(notificationPreferences.quiet_hours_start, notificationPreferences.quiet_hours_end)) {
      const sound = notificationPreferences.sound_source === 'custom' ? customNotificationSound : null;
      const tone = strategy.kind === 'direct' ? 'direct-call' : 'group-call';
      const cadence = sound && strategy.repeatMs !== null
        ? { ...strategy, repeatMs: Math.max(strategy.repeatMs, Math.ceil(sound.duration * 1000) + 500) }
        : strategy;
      if (strategy.kind === 'direct') stopDirectCallRingtone?.();
      const stop = startCallAlertCadence(cadence, () => playNotificationSound(sound, notificationPreferences.sound_volume ?? 70, tone), elapsedMs);
      if (strategy.kind === 'direct') {
        ringingDirectCallKey = alertKey;
        stopDirectCallRingtone = stop;
      }
    }
    if (!notificationPreferences.browser_enabled || !('Notification' in window) || Notification.permission !== 'granted') return;
    const notification = new Notification(strategy.title, {
      body: strategy.body,
      icon: deployment.branding.logo_url || '/images/system-icon.png',
      tag: `mx-call-${alertKey}`,
      requireInteraction: strategy.requireInteraction,
      silent: true
    });
    callBrowserNotifications.set(alertKey, notification);
    notification.onclick = () => { window.focus(); stopIncomingCallAlert(call.channel_uid); location.hash = `collaboration?channel=${encodeURIComponent(call.channel_uid)}`; };
    notification.onclose = () => callBrowserNotifications.delete(alertKey);
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
  function notificationMatchesOpenContext(item: MxNotification) {
    return view === 'collaboration'
      && document.visibilityState === 'visible'
      && document.hasFocus()
      && activeCollaborationNewestVisible
      && item.target_type === 'channel'
      && !!item.target_uid
      && item.target_uid === activeCollaborationChannelUid;
  }
  async function acknowledgeContextNotification(item: MxNotification) {
    dismissToast(item.uid);
    try { await markNotificationRead(item.uid); }
    catch { /* The next inbox refresh will retain it if the acknowledgement failed. */ }
    notificationRevision += 1;
    await refreshUnreadCount();
  }
  function announceNotification(message: LiveMessage) {
    const item = asNotification(message);
    if (!item) { void refreshUnreadCount(); return; }
    if (notificationMatchesOpenContext(item)) { void acknowledgeContextNotification(item); return; }
    unreadNotifications += 1;
    if (!alertImmediately() || inQuietHours(notificationPreferences.quiet_hours_start, notificationPreferences.quiet_hours_end)) return;
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
  function setActiveCollaborationChannel(uid: string, newestVisible: boolean) {
    activeCollaborationChannelUid = uid;
    activeCollaborationNewestVisible = newestVisible;
    if (!newestVisible || view !== 'collaboration' || document.visibilityState !== 'visible' || !document.hasFocus()) return;
    for (const item of notificationToasts.filter((toast) => toast.target_type === 'channel' && toast.target_uid === uid)) void acknowledgeContextNotification(item);
  }
  function openNotification(item: MxNotification) {
    dismissToast(item.uid);
    if (!item.read_at) { item.read_at = Date.now(); unreadNotifications = Math.max(0, unreadNotifications - 1); notificationRevision += 1; void markNotificationRead(item.uid); }
    const recordIntent = recordNotificationIntent(item);
    if (recordIntent) {
      selectedModuleUid = recordIntent.moduleUid;
      linkedRecordUid = recordIntent.recordUid;
      linkedRecordFocusAttachments = recordIntent.focusAttachments;
      linkedRecordRequestRevision += 1;
      view = 'records';
      mobileOpen = false;
      location.hash = `module/${encodeURIComponent(recordIntent.moduleUid)}?record=${encodeURIComponent(recordIntent.recordUid)}${recordIntent.focusAttachments ? '&attachments=1' : ''}`;
    }
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
    else if (view === 'drive') dashboardRevision += 1;
    else if (view === 'admin') adminRevision += 1;
  }
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
    const fontPixels = Math.min(24, Math.max(10, Number(fontScale) || 16));
    const font = fontPixels / 16 * 100;
    // Typography and interface density are separate preferences. Previously
    // both values were multiplied into the root font size, making the two
    // controls behave identically.
    document.documentElement.style.removeProperty('font-size');
    for (const legacyProperty of ['--interface-scale', '--interface-viewport-width', '--interface-viewport-height']) document.documentElement.style.removeProperty(legacyProperty);
    const { interfaceRatio, fontRatio } = appearanceScale(value, font);
    document.documentElement.style.setProperty('--interface-factor', String(interfaceRatio));
    document.documentElement.style.setProperty('--user-interface-factor', String(interfaceRatio));
    document.documentElement.style.setProperty('--font-scale', String(fontRatio));
    document.documentElement.style.setProperty('--application-font-scale', String(fontRatio));
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
    for (const property of ['--interface-factor', '--font-scale', '--application-font-scale']) document.documentElement.style.removeProperty(property);
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
      fontScale = preferences.font_size_px
        ?? Math.round(16 * (preferences.font_scale_percent ?? 100) / 100);
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
    fontScale = Math.min(24, Math.max(10, Number(fontScale) || 16));
    autoRefresh = Math.max(0, Number(autoRefresh) || 0);
    settingsBusy = true; settingsError = '';
    try {
      const current = await loadPreferences();
      await savePreferences({
        ...current.preferences,
        theme: (theme || null) as UserPreferences['theme'], auto_scale: autoScale,
        accent_color: validAccentColor(accentColor) ? accentColor : null,
        ui_scale_percent: scale, font_size_px: fontScale, font_scale_percent: null,
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
  <WorkspaceSidebar {deployment} {session} {modules} {view} {selectedModuleUid} {selectedModule} {live} open={mobileOpen} onToggle={() => mobileOpen = !mobileOpen} onNavigate={go} onNavigateModule={goModule} onExport={() => exportReport('records', undefined, '', '', selectedModule?.uid || 'mx-default-records')} />
  <main class="main-stage">
    <WorkspaceTopbar productName={deployment.branding.display_name} {viewTitle} {live} navigationOpen={mobileOpen} {notificationRevision} {unreadNotifications} onToggleNavigation={() => mobileOpen = !mobileOpen} onRefresh={refreshCurrent} onOpenSettings={() => settingsOpen = true} onToggleTheme={toggleTheme} onOpenAccount={() => go('account')} onSignOut={() => onSessionEnded('Signed out.')} onUnread={(count) => unreadNotifications = count} onOpenNotification={openNotification} />
    {#if view === 'dashboard'}<DashboardView isAdmin={session.access_level === 0} revision={dashboardRevision} dashboardLabel={deployment.terminology.dashboard_label} recordSingular={deployment.terminology.record_singular} recordPlural={deployment.terminology.record_plural} />
    {:else if view === 'records' && selectedModule}{#key selectedModuleUid}<RecordsView {session} accessLevel={session.access_level} moduleUid={selectedModule.uid} modulePermission={selectedModulePermission} openRecordUid={linkedRecordUid} openRequestRevision={linkedRecordRequestRevision} focusLinkedAttachments={linkedRecordFocusAttachments} revision={recordRevision} liveMessage={lastRecordEvent} recordSingular={selectedModule.singular_name} recordPlural={selectedModule.name} />{/key}
    {:else if view === 'records'}<section class="panel module-access-empty"><h2>No module access</h2><p>Your account has not been granted access to any active record module. Ask an MX administrator to assign the modules needed for your work.</p><button class="button" type="button" onclick={() => go('collaboration')}>Open collaboration</button></section>
    {:else if view === 'collaboration'}<CollaborationView {session} openChannelUid={linkedChannelUid} revision={collaborationRevision} connected={live} liveEvent={lastCollaborationEvent} {callEvents} activeCalls={ongoingCalls} activeCallChannelUid={activeCall?.channelUid || ''} onStartCall={startCollaborationCall} onChannelChanged={setActiveCollaborationChannel} />
    {:else if view === 'drive'}<DriveView accountUid={session.uid} revision={dashboardRevision} uploads={driveUploads} />
    {:else if view === 'admin' && session.access_level === 0}{#key adminRevision}<AdminView {session} administrationLabel={deployment.terminology.administration_label} dashboardLabel={deployment.terminology.dashboard_label} />{/key}
    {:else}<AccountSecurity {session} {onSessionChanged} {onSessionEnded} />{/if}
  </main>
  {#if mobileOpen}<button class="sidebar-scrim" aria-label="Close menu" onclick={() => mobileOpen = false}></button>{/if}
</div>

<DriveUploadPanel uploads={driveUploads} avoidComposer={view === 'collaboration'} />

{#if incomingCall}
  {@const caller = incomingCall.participants[0]}
  {@const privateCall = incomingCall.channel_kind === 'direct'}
  <div class:direct-call={privateCall} class:group-call={!privateCall} class="incoming-call-card" role="dialog" tabindex="-1" aria-live={privateCall ? 'assertive' : 'polite'} aria-label={privateCall ? `Incoming private call from ${caller?.user_name || incomingCall.channel_name}` : `Group call in ${incomingCall.channel_name}`}>
    <header><span class="incoming-call-icon">{#if privateCall}<PhoneIncoming size={20} />{:else}<UsersRound size={20} />{/if}</span><span><small>{privateCall ? 'Incoming private call' : 'Group call available'}</small><strong>{privateCall ? (caller?.user_name || incomingCall.channel_name) : incomingCall.channel_name}</strong><em>{privateCall ? `${incomingCall.mode === 'video' ? 'Video' : 'Voice'} call` : `${caller?.user_name || 'A member'} started it · ${incomingCall.participants.length} connected`}</em></span><button type="button" onclick={() => dismissIncomingCall(incomingCall.channel_uid)} aria-label={privateCall ? 'Decline private call' : 'Dismiss group call notification'}><X size={16} /></button></header>
    <footer><button class:danger={privateCall} class="button incoming-call-dismiss" type="button" onclick={() => dismissIncomingCall(incomingCall.channel_uid)}><PhoneOff size={15} /> {privateCall ? 'Decline' : 'Not now'}</button><button class="button" type="button" onclick={() => void answerOngoingCall(incomingCall, 'voice')}><Phone size={15} /> {privateCall ? 'Answer voice' : 'Join voice'}</button><button class="button primary" type="button" onclick={() => void answerOngoingCall(incomingCall, 'video')}><Video size={15} /> {privateCall ? 'Answer video' : 'Join video'}</button></footer>
  </div>
{/if}

{#if activeCall}
  {#key activeCall.nonce}<CollaborationCall {session} channelUid={activeCall.channelUid} channelName={activeCall.channelName} channelKind={activeCall.channelKind} initialMode={activeCall.mode} connected={live} {callEvents} onEnded={endLocalCall} />{/key}
{/if}

{#if callSessionNotice}<div class="call-session-notice" role="status"><span>{callSessionNotice}</span><button type="button" onclick={() => callSessionNotice = ''} aria-label="Dismiss call notice"><X size={15} /></button></div>{/if}

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

{#if settingsOpen}
  <DisplaySettingsDialog
    {deployment}
    error={settingsError}
    busy={settingsBusy}
    {accentPresets}
    bind:theme
    bind:accentColor
    bind:density
    bind:contentWidth
    bind:autoScale
    bind:scale
    bind:fontScale
    bind:reducedMotion
    bind:autoRefresh
    {recommendedScale}
    onClose={() => settingsOpen = false}
    onThemeChange={setTheme}
    onAppearanceChange={applyPersonalAppearance}
    onScaleChange={applyScale}
    onSave={saveDisplaySettings}
  />
{/if}
