<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import FileText from '@lucide/svelte/icons/file-text';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import Search from '@lucide/svelte/icons/search';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Blocks from '@lucide/svelte/icons/blocks';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import type { DeploymentConfig, ModuleDefinition } from '../api/domain';
  import type { Session } from '../api/types';
  import GlobalSearch from './GlobalSearch.svelte';
  import ModuleIcon from './ModuleIcon.svelte';
  import ProfileAvatar from './ProfileAvatar.svelte';
  import ModuleBrowser from './ModuleBrowser.svelte';
  import { matchingModules, orderedModules } from '../util/moduleNavigation';

  type View = 'dashboard' | 'records' | 'collaboration' | 'drive' | 'admin' | 'account';

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
  let moduleBrowserOpen = $state(false), modulesExpanded = $state(false), moduleQuery = $state('');
  const availableModules = $derived(orderedModules(modules));
  const shortcutMatches = $derived(matchingModules(availableModules, moduleQuery));
  const shortcuts = $derived(shortcutMatches.slice(0, 12));

  function chooseModule(uid: string) { moduleBrowserOpen = false; onNavigateModule(uid); }

  function openSearch() {
    if (!open) onToggle();
    window.requestAnimationFrame(() => searchRegion?.querySelector('input')?.focus());
  }

  function initials(name: string) {
    return name.split(/\s+/).map((part) => part[0]).join('').slice(0, 2).toUpperCase();
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
    {#if availableModules.length || deployment.navigation.show_records}
      <button class:active={view === 'records'} class:expanded-trigger={open} title="Modules" aria-label="Modules" aria-expanded={open ? modulesExpanded : undefined} aria-haspopup={!open ? 'dialog' : undefined} aria-controls={open && modulesExpanded ? 'workspace-module-shortcuts' : undefined} onclick={() => { if (open) modulesExpanded = !modulesExpanded; else moduleBrowserOpen = true; }}>
        <span><Blocks size={18} /></span><b>Modules</b>{#if open}{#if modulesExpanded}<ChevronDown size={15}/>{:else}<ChevronRight size={15}/>{/if}{/if}
      </button>
      {#if open && modulesExpanded}
        <section id="workspace-module-shortcuts" class="module-shortcuts" aria-label="Module shortcuts">
          <button type="button" class="browse-modules" onclick={() => moduleBrowserOpen = true}><span><Search size={16}/></span><b>Browse all modules ({availableModules.length})</b></button>
          {#if availableModules.length}<input type="search" aria-label="Filter module shortcuts" placeholder="Find a module…" bind:value={moduleQuery}/>{/if}
          <div class="module-shortcut-list">
            {#each shortcuts as module (module.uid)}<button class:active={view === 'records' && selectedModuleUid === module.uid} title={module.name} onclick={() => chooseModule(module.uid)}><span style={`color:${module.color}`}><ModuleIcon name={module.icon} size={17}/></span><b>{module.name}</b></button>{/each}
          </div>
          {#if !shortcutMatches.length}<p>{availableModules.length ? 'No matching modules.' : 'No modules available yet.'}</p>{:else if shortcutMatches.length > shortcuts.length}<p>Showing {shortcuts.length} of {shortcutMatches.length}. Browse all to see more.</p>{/if}
        </section>
      {/if}
    {/if}
    <button class:active={view === 'collaboration'} title="Collaboration" onclick={() => onNavigate('collaboration')}>
      <span><MessagesSquare size={18} /></span><b>Collaboration</b>
    </button>
    <button class:active={view === 'drive'} title="MX Drive" onclick={() => onNavigate('drive')}><span><HardDrive size={18} /></span><b>MX Drive</b></button>
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

{#if moduleBrowserOpen}<ModuleBrowser modules={availableModules} selectedModuleUid={view === 'records' ? selectedModuleUid : ''} onChoose={chooseModule} onClose={() => moduleBrowserOpen = false}/>{/if}

<style>
  .main-nav .expanded-trigger{grid-template-columns:2.35rem minmax(0,1fr) 1.25rem;padding-right:.35rem;}
  .module-shortcuts{min-width:0;border-left:1px solid color-mix(in srgb,var(--on-sidebar) 18%,transparent);margin:.2rem .25rem .5rem .7rem;padding-left:.35rem;}
  .module-shortcuts input{width:100%;min-width:0;box-sizing:border-box;font-size:.8rem;margin:.3rem 0;background:color-mix(in srgb,var(--on-sidebar) 6%,transparent);color:var(--on-sidebar);border:1px solid color-mix(in srgb,var(--on-sidebar) 22%,transparent);}
  .module-shortcuts input::placeholder{color:color-mix(in srgb,var(--on-sidebar) 60%,transparent);}.module-shortcut-list{max-height:30dvh;min-height:0;overflow:auto;overscroll-behavior:contain;}.module-shortcuts p{font-size:.75rem;line-height:1.4;color:color-mix(in srgb,var(--on-sidebar) 65%,transparent);margin:.5rem .3rem;}
</style>
