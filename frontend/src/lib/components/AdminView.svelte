<script lang="ts">
  import { onMount } from 'svelte';
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
  <nav class="subnav" aria-label={`${administrationLabel} sections`}><button class:active={tab === 'accounts'} onclick={() => setTab('accounts')}>Accounts</button><button class:active={tab === 'schema'} onclick={() => setTab('schema')}>Modules & fields</button><button class:active={tab === 'dashboard'} onclick={() => setTab('dashboard')}>{dashboardLabel}</button><button class:active={tab === 'trash'} onclick={() => setTab('trash')}>Trash</button><button class:active={tab === 'deployment'} onclick={() => setTab('deployment')}>Deployment</button><button class:active={tab === 'backups'} onclick={() => setTab('backups')}>Backups</button><button class:active={tab === 'updates'} onclick={() => setTab('updates')}>Updates</button><button class:active={tab === 'audit'} onclick={() => setTab('audit')}>Audit</button></nav>
  {#if tab === 'accounts'}<AccountsAdmin {session} />{:else if tab === 'schema'}<SchemaAdmin />{:else if tab === 'dashboard'}<DashboardAdmin />{:else if tab === 'trash'}<TrashAdmin />{:else}<OperationsAdmin section={tab} />{/if}
</section>
