<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from '@lucide/svelte/icons/bell';
  import CheckCheck from '@lucide/svelte/icons/check-check';
  import X from '@lucide/svelte/icons/x';
  import type { MxNotification } from '../api/domain';
  import { loadNotifications, markAllNotificationsRead, markNotificationRead } from '../api/workspace';
  import { notificationPresentation } from '../util/notificationKinds';
  import NotificationKindIcon from './NotificationKindIcon.svelte';

  let { revision = 0, onUnread = (_count: number) => undefined, onClose = () => undefined }:
    { revision?: number; onUnread?: (count: number) => void; onClose?: () => void } = $props();
  let notifications = $state<MxNotification[]>([]);
  let unread = $state(0);
  let loading = $state(true);
  let error = $state('');

  onMount(() => void refresh());
  $effect(() => { revision; if (!loading) void refresh(); });

  async function refresh() {
    try {
      const response = await loadNotifications();
      notifications = response.notifications;
      unread = response.unread;
      onUnread(unread);
      error = '';
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'Could not load notifications.';
    } finally { loading = false; }
  }

  async function read(item: MxNotification) {
    if (!item.read_at) {
      await markNotificationRead(item.uid);
      item.read_at = Date.now();
      unread = Math.max(0, unread - 1);
      onUnread(unread);
    }
    if (item.target_type === 'record' && item.module_uid && item.target_uid) {
      location.hash = `module/${encodeURIComponent(item.module_uid)}?record=${encodeURIComponent(item.target_uid)}`;
      onClose();
    } else if (item.target_type === 'channel' && item.target_uid) {
      location.hash = `collaboration?channel=${encodeURIComponent(item.target_uid)}`;
      onClose();
    } else if (item.module_uid) {
      location.hash = `module/${encodeURIComponent(item.module_uid)}`;
      onClose();
    }
  }

  async function readAll() {
    await markAllNotificationsRead();
    for (const item of notifications) item.read_at ||= Date.now();
    unread = 0;
    onUnread(0);
  }

  function when(value: number) {
    return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(value);
  }
</script>

<div class="notification-popover" role="dialog" aria-label="Notifications">
  <header><div class="notification-title"><span><Bell size={17} /></span><span><strong>Notifications</strong><small>{unread ? `${unread} unread` : 'You are caught up'}</small></span></div><div class="inline-actions">{#if unread}<button class="link-button icon-label" onclick={readAll}><CheckCheck size={14} />Read all</button>{/if}<button class="icon-button" aria-label="Close notifications" onclick={onClose}><X size={17} /></button></div></header>
  {#if error}<div class="notice error compact">{error}</div>{/if}
  <div class="notification-list">
    {#if loading}<p class="muted">Loading notifications…</p>
    {:else if !notifications.length}<div class="empty-state compact"><strong>No notifications yet</strong><p>Messages, mentions, assignments, files, and subscribed record activity appear here.</p></div>
    {:else}{#each notifications as item}
      {@const presentation = notificationPresentation(item)}
      <button class:unread={!item.read_at} class="notification-item" data-category={presentation.category} onclick={() => read(item)}>
        <span class="notification-kind-icon"><NotificationKindIcon category={presentation.category} size={17} /></span>
        <span class="notification-item-content">
          <span class="notification-item-meta"><b>{presentation.label}</b>{#if !item.read_at}<i>New</i>{/if}</span>
          <strong>{item.title}</strong>
          {#if item.body}<small>{item.body}</small>{/if}
          <em>{item.actor_name ? `${item.actor_name} · ` : ''}{when(item.created_at)}</em>
        </span>
      </button>
    {/each}{/if}
  </div>
</div>
