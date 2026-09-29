<script lang="ts">
  import { onMount } from 'svelte';
  import Archive from '@lucide/svelte/icons/archive';
  import Blocks from '@lucide/svelte/icons/blocks';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import ServerCog from '@lucide/svelte/icons/server-cog';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import type { Session } from '../api/types';
  import AccountsAdmin from './admin/AccountsAdmin.svelte';
  import DashboardAdmin from './admin/DashboardAdmin.svelte';
  import OperationsAdmin from './admin/OperationsAdmin.svelte';
  import SchemaAdmin from './admin/SchemaAdmin.svelte';
  import TrashAdmin from './admin/TrashAdmin.svelte';
  let { session, administrationLabel = 'Administration', dashboardLabel = 'Dashboard' }: { session: Session; administrationLabel?: string; dashboardLabel?: string } = $props();
  let tab = $state<'accounts' | 'schema' | 'dashboard' | 'trash' | 'deployment' | 'backups' | 'updates' | 'audit'>('accounts');
  onMount(() => { const stored = localStorage.getItem('mx_admin_tab'); if (stored === 'operations') tab = 'deployment'; else if (stored === 'accounts' || stored === 'schema' || stored === 'dashboard' || stored === 'trash' || stored === 'deployment' || stored === 'backups' || stored === 'updates' || stored === 'audit') tab = stored; });
  function setTab(next: typeof tab) { tab = next; localStorage.setItem('mx_admin_tab', next); }
</script>

<section class="workspace-page admin-page">
  <nav class="subnav admin-subnav" aria-label={`${administrationLabel} sections`}><button class:active={tab === 'accounts'} onclick={() => setTab('accounts')} title="Accounts"><UsersRound size={15} /><span>Accounts</span></button><button class:active={tab === 'schema'} onclick={() => setTab('schema')} title="Modules and fields"><Blocks size={15} /><span>Modules & fields</span></button><button class:active={tab === 'dashboard'} onclick={() => setTab('dashboard')} title={dashboardLabel}><LayoutDashboard size={15} /><span>{dashboardLabel}</span></button><button class:active={tab === 'trash'} onclick={() => setTab('trash')} title="Trash"><Trash2 size={15} /><span>Trash</span></button><button class:active={tab === 'deployment'} onclick={() => setTab('deployment')} title="Deployment"><ServerCog size={15} /><span>Deployment</span></button><button class:active={tab === 'backups'} onclick={() => setTab('backups')} title="Backups"><Archive size={15} /><span>Backups</span></button><button class:active={tab === 'updates'} onclick={() => setTab('updates')} title="Updates"><RefreshCw size={15} /><span>Updates</span></button><button class:active={tab === 'audit'} onclick={() => setTab('audit')} title="Audit"><ScrollText size={15} /><span>Audit</span></button></nav>
  <div class="admin-content">
    {#if tab === 'accounts'}<AccountsAdmin {session} />{:else if tab === 'schema'}<SchemaAdmin {session} />{:else if tab === 'dashboard'}<DashboardAdmin />{:else if tab === 'trash'}<TrashAdmin />{:else}<OperationsAdmin section={tab} />{/if}
  </div>
</section>
