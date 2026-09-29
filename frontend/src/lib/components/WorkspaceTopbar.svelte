<script lang="ts">
  import Bell from '@lucide/svelte/icons/bell';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Menu from '@lucide/svelte/icons/menu';
  import MoonStar from '@lucide/svelte/icons/moon-star';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Settings from '@lucide/svelte/icons/settings';
  import UserRound from '@lucide/svelte/icons/user-round';
  import NotificationCenter from './NotificationCenter.svelte';

  let {
    productName,
    viewTitle,
    live,
    navigationOpen,
    notificationRevision,
    unreadNotifications,
    onToggleNavigation,
    onRefresh,
    onOpenSettings,
    onToggleTheme,
    onOpenAccount,
    onSignOut,
    onUnread
  }: {
    productName: string;
    viewTitle: string;
    live: boolean;
    navigationOpen: boolean;
    notificationRevision: number;
    unreadNotifications: number;
    onToggleNavigation: () => void;
    onRefresh: () => void;
    onOpenSettings: () => void;
    onToggleTheme: () => void;
    onOpenAccount: () => void;
    onSignOut: () => void;
    onUnread: (count: number) => void;
  } = $props();

  let notificationsOpen = $state(false);
</script>

<header class="workspace-topbar">
  <div class="topbar-context">
    <button class="topbar-tool navigation-trigger" type="button" title="Navigation" aria-label="Open navigation" aria-expanded={navigationOpen} onclick={onToggleNavigation}><Menu size={18} /></button>
    <div class="workspace-identity"><small>{productName}</small><strong>{viewTitle}</strong></div>
  </div>
  <div class="desktop-actions">
    <span class:online={live} class="topbar-live" title={live ? 'Live sync connected' : 'Reconnecting'}><i></i><span>{live ? 'Live' : 'Reconnecting'}</span></span>
    <button class="topbar-tool refresh-button" type="button" title="Refresh current workspace" aria-label="Refresh current workspace" onclick={onRefresh}><RefreshCw size={17} /></button>
    <div class="notification-anchor">
      <button class="topbar-tool notification-button" type="button" title="Notifications" aria-label={`Notifications${unreadNotifications ? `, ${unreadNotifications} unread` : ''}`} aria-expanded={notificationsOpen} onclick={() => notificationsOpen = !notificationsOpen}>
        <Bell size={17} />{#if unreadNotifications}<span>{unreadNotifications > 99 ? '99+' : unreadNotifications}</span>{/if}
      </button>
      {#if notificationsOpen}<NotificationCenter revision={notificationRevision} onUnread={onUnread} onClose={() => notificationsOpen = false} />{/if}
    </div>
    <span class="topbar-divider"></span>
    <button class="topbar-tool settings-button" type="button" title="Personal settings" aria-label="Personal settings" onclick={onOpenSettings}><Settings size={17} /></button>
    <button class="topbar-tool theme-button" type="button" title="Change theme" aria-label="Change theme" onclick={onToggleTheme}><MoonStar size={17} /></button>
    <button class="topbar-tool account-button" type="button" title="My account" aria-label="My account" onclick={onOpenAccount}><UserRound size={17} /></button>
    <button class="topbar-tool sign-out-button" type="button" title="Sign out" aria-label="Sign out" onclick={onSignOut}><LogOut size={17} /></button>
  </div>
</header>
