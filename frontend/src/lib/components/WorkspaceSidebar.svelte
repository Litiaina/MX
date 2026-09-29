<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import FileText from '@lucide/svelte/icons/file-text';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import Search from '@lucide/svelte/icons/search';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import type { DeploymentConfig, ModuleDefinition } from '../api/domain';
  import type { Session } from '../api/types';
  import GlobalSearch from './GlobalSearch.svelte';
  import ModuleIcon from './ModuleIcon.svelte';
  import ProfileAvatar from './ProfileAvatar.svelte';

  type View = 'dashboard' | 'records' | 'collaboration' | 'admin' | 'account';

  let {
    deployment,
    session,
    modules,
    view,
    selectedModuleUid,
    selectedModule,
    live,
    open,
    onToggle,
    onNavigate,
    onNavigateModule,
    onExport
  }: {
    deployment: DeploymentConfig;
    session: Session;
    modules: ModuleDefinition[];
    view: View;
    selectedModuleUid: string;
    selectedModule: ModuleDefinition | null;
    live: boolean;
    open: boolean;
    onToggle: () => void;
    onNavigate: (view: View) => void;
    onNavigateModule: (uid: string) => void;
    onExport: () => void;
  } = $props();

  let searchRegion: HTMLDivElement;

  function openSearch() {
    if (!open) onToggle();
    window.requestAnimationFrame(() => searchRegion?.querySelector('input')?.focus());
  }

  function initials(name: string) {
    return name.split(/\s+/).map((part) => part[0]).join('').slice(0, 2).toUpperCase();
  }

  function moduleGroup(module: ModuleDefinition) {
    return typeof module.config.navigation_group === 'string' ? module.config.navigation_group.trim() : '';
  }
</script>

<aside class:open class="sidebar" aria-label="Workspace navigation">
  <header class="sidebar-brand">
    {#if deployment.branding.logo_url}
      <img src={deployment.branding.logo_url} alt="" />
    {:else}
      <span>{initials(deployment.branding.display_name)}</span>
    {/if}
    <div>
      <strong>{deployment.branding.display_name}</strong>
      <small>{deployment.branding.subtitle}</small>
    </div>
    <button class="sidebar-toggle" type="button" aria-label={open ? 'Collapse navigation' : 'Expand navigation'} aria-expanded={open} onclick={onToggle}>
      {#if open}<PanelLeftClose size={17} />{:else}<PanelLeftOpen size={17} />{/if}
    </button>
  </header>

  <div class="sidebar-search-region">
    <button class="sidebar-search-trigger" type="button" title="Search all modules" aria-label="Expand navigation to search" onclick={openSearch}><Search size={17} /></button>
    <div class="sidebar-search-field" bind:this={searchRegion}><GlobalSearch onRequestOpen={openSearch} /></div>
  </div>

  <nav class="main-nav" aria-label="Main navigation">
    {#if deployment.navigation.show_dashboard}
      <button class:active={view === 'dashboard'} title={deployment.terminology.dashboard_label} onclick={() => onNavigate('dashboard')}>
        <span><LayoutDashboard size={18} /></span><b>{deployment.terminology.dashboard_label}</b>
      </button>
    {/if}
    {#if modules.length}
      {#each modules as module, index}
        {#if moduleGroup(module) && (index === 0 || moduleGroup(modules[index - 1]) !== moduleGroup(module))}
          <span class="nav-group-label">{moduleGroup(module)}</span>
        {/if}
        <button class:active={view === 'records' && selectedModuleUid === module.uid} title={module.name} onclick={() => onNavigateModule(module.uid)}>
          <span style={`color:${module.color}`}><ModuleIcon name={module.icon} size={18} /></span><b>{module.name}</b>
        </button>
      {/each}
    {:else if deployment.navigation.show_records}
      <button class:active={view === 'records'} title={deployment.terminology.record_plural} onclick={() => onNavigate('records')}>
        <span><FileText size={18} /></span><b>{deployment.terminology.record_plural}</b>
      </button>
    {/if}
    <button class:active={view === 'collaboration'} title="Collaboration" onclick={() => onNavigate('collaboration')}>
      <span><MessagesSquare size={18} /></span><b>Collaboration</b>
    </button>
    {#if session.access_level === 0}
      <button class:active={view === 'admin'} title={deployment.terminology.administration_label} onclick={() => onNavigate('admin')}>
        <span><ShieldCheck size={18} /></span><b>{deployment.terminology.administration_label}</b>
      </button>
    {/if}
  </nav>

  {#if deployment.navigation.show_quick_actions}
    <section class="quick-actions">
      <span>Quick actions</span>
      {#if deployment.navigation.show_records}
        <button title={`Open ${(selectedModule?.singular_name || deployment.terminology.record_singular).toLowerCase()} workspace`} onclick={() => onNavigateModule(selectedModule?.uid || modules[0]?.uid || 'mx-default-records')}>
          <i><FileText size={16} /></i><b>Open workspace</b>
        </button>
      {/if}
      <button title="Export detailed records" onclick={onExport}>
        <i><Download size={16} /></i><b>Export records</b>
      </button>
    </section>
  {/if}

  <footer class="sidebar-foot">
    <div class="live-state" title={live ? 'Live sync connected' : 'Reconnecting live sync'}><i class:online={live}></i><span>{live ? 'Live sync connected' : 'Reconnecting live sync'}</span></div>
    <button class:active={view === 'account'} class="profile-button" title={`${session.name} · My account`} onclick={() => onNavigate('account')}>
      <span><ProfileAvatar userUid={session.uid} name={session.name} updatedAt={session.profile_photo_updated_at} /></span><div><strong>{session.name}</strong><small>{session.access_name} · My account</small></div>
    </button>
  </footer>
</aside>
