<script lang="ts">
  import { tick } from 'svelte';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
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
    onUnread,
    onOpenNotification
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
    onOpenNotification: (item: import('../api/domain').MxNotification) => void;
  } = $props();

  let notificationsOpen = $state(false);
  let moreOpen = $state(false);
  let moreButton: HTMLButtonElement;
  let moreMenu = $state<HTMLDivElement>();
  let notificationAnchor = $state<HTMLDivElement>();

  function dismissNotifications(event: PointerEvent) {
    if (moreOpen && event.target instanceof Element && !event.target.closest('.topbar-more')) moreOpen=false;
    if (!notificationsOpen || !(event.target instanceof Node) || notificationAnchor?.contains(event.target)) return;
    notificationsOpen = false;
  }
  function notificationKeydown(event: KeyboardEvent) {
    if(moreOpen&&event.key==='Escape'){event.preventDefault();moreOpen=false;moreButton?.focus();return;}
    if (notificationsOpen && event.key === 'Escape') { event.preventDefault(); notificationsOpen = false; }
  }
  async function toggleMore(){moreOpen=!moreOpen;if(moreOpen){await tick();moreMenu?.querySelector<HTMLButtonElement>('button')?.focus();}}
  function moreKeydown(event:KeyboardEvent){
    if(event.key==='Tab'){moreOpen=false;return;}
    if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return;
    event.preventDefault();const items=[...moreMenu?.querySelectorAll<HTMLButtonElement>('button')||[]];const index=items.indexOf(document.activeElement as HTMLButtonElement);
    items[event.key==='Home'?0:event.key==='End'?items.length-1:(index+(event.key==='ArrowUp'?-1:1)+items.length)%items.length]?.focus();
  }
  function moreAction(action:()=>void){moreOpen=false;moreButton?.focus();action();}
</script>

<svelte:window onpointerdown={dismissNotifications} onkeydown={notificationKeydown} />

<header class="workspace-topbar">
  <div class="topbar-context">
    <button class="topbar-tool navigation-trigger" type="button" title="Navigation" aria-label="Open navigation" aria-expanded={navigationOpen} onclick={onToggleNavigation}><Menu size={18} /></button>
    <div class="workspace-identity"><small>{productName}</small><strong>{viewTitle}</strong></div>
  </div>
  <div class="desktop-actions">
    <span class:online={live} class="topbar-live" title={live ? 'Live sync connected' : 'Reconnecting'}><i></i><span>{live ? 'Live' : 'Reconnecting'}</span></span>
    <button class="topbar-tool refresh-button secondary-tool" type="button" title="Refresh current workspace" aria-label="Refresh current workspace" onclick={onRefresh}><RefreshCw size={17} /></button>
    <div class="notification-anchor" bind:this={notificationAnchor}>
      <button class="topbar-tool notification-button" type="button" title="Notifications" aria-label={`Notifications${unreadNotifications ? `, ${unreadNotifications} unread` : ''}`} aria-expanded={notificationsOpen} onclick={() => notificationsOpen = !notificationsOpen}>
        <Bell size={17} />{#if unreadNotifications}<span>{unreadNotifications > 99 ? '99+' : unreadNotifications}</span>{/if}
      </button>
      {#if notificationsOpen}<NotificationCenter revision={notificationRevision} onUnread={onUnread} onOpen={onOpenNotification} onClose={() => notificationsOpen = false} />{/if}
    </div>
    <span class="topbar-divider"></span>
    <button class="topbar-tool settings-button" type="button" title="Personal settings" aria-label="Personal settings" onclick={onOpenSettings}><Settings size={17} /></button>
    <button class="topbar-tool theme-button secondary-tool" type="button" title="Change theme" aria-label="Change theme" onclick={onToggleTheme}><MoonStar size={17} /></button>
    <button class="topbar-tool account-button secondary-tool" type="button" title="My account" aria-label="My account" onclick={onOpenAccount}><UserRound size={17} /></button>
    <button class="topbar-tool sign-out-button secondary-tool" type="button" title="Sign out" aria-label="Sign out" onclick={onSignOut}><LogOut size={17} /></button>
    <div class="topbar-more"><button bind:this={moreButton} class="topbar-tool" type="button" aria-label="More workspace actions" aria-haspopup="menu" aria-expanded={moreOpen} onclick={()=>void toggleMore()}><Ellipsis size={19}/></button>
      {#if moreOpen}<div bind:this={moreMenu} class="topbar-more-menu" role="menu" aria-label="Workspace actions" tabindex="-1" onkeydown={moreKeydown}><button type="button" role="menuitem" onclick={()=>moreAction(onRefresh)}><RefreshCw size={17}/>Refresh workspace</button><button type="button" role="menuitem" onclick={()=>moreAction(onToggleTheme)}><MoonStar size={17}/>Change theme</button><button type="button" role="menuitem" onclick={()=>moreAction(onOpenAccount)}><UserRound size={17}/>My account</button><button type="button" role="menuitem" onclick={()=>moreAction(onSignOut)}><LogOut size={17}/>Sign out</button></div>{/if}
    </div>
  </div>
</header>

<style>
  .workspace-topbar{container:mx-topbar / inline-size;}
  .topbar-more{display:none;position:relative;}
  .topbar-more-menu{position:absolute;right:0;top:calc(100% + .5rem);z-index:100;width:min(17rem,calc(100vw - 1rem));padding:.35rem;border:1px solid var(--line);border-radius:.5rem;background:var(--surface);box-shadow:var(--shadow-popover);}
  .topbar-more-menu button{display:flex;align-items:center;gap:.7rem;width:100%;padding:.7rem .8rem;min-height:2.5rem;font:inherit;font-size:.85rem;text-align:left;color:var(--text);border:0;border-radius:.35rem;background:transparent;cursor:pointer;}.topbar-more-menu button:hover,.topbar-more-menu button:focus-visible{background:var(--surface-2);}.topbar-more-menu button :global(svg){flex-shrink:0;}
  @container mx-topbar (max-width:34rem){.secondary-tool,.topbar-live,.topbar-divider{display:none;}.topbar-more{display:block;}.desktop-actions{flex:0 0 auto;gap:.15rem;}.topbar-context{flex:1;min-width:0;gap:.35rem;}.workspace-identity{min-width:0;}.workspace-identity strong,.workspace-identity small{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}}
</style>
