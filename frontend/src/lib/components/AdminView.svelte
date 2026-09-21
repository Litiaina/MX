<script lang="ts">
  import { onMount } from 'svelte';
  import type { Session } from '../api/types';
  import AccountsAdmin from './admin/AccountsAdmin.svelte';
  import DashboardAdmin from './admin/DashboardAdmin.svelte';
  import OperationsAdmin from './admin/OperationsAdmin.svelte';
  import SchemaAdmin from './admin/SchemaAdmin.svelte';
  let { session, administrationLabel = 'Administration', dashboardLabel = 'Dashboard' }: { session: Session; administrationLabel?: string; dashboardLabel?: string } = $props();
  let tab = $state<'accounts' | 'schema' | 'dashboard' | 'operations'>('accounts');
  onMount(() => { const stored = localStorage.getItem('mx_admin_tab'); if (stored === 'accounts' || stored === 'schema' || stored === 'dashboard' || stored === 'operations') tab = stored; });
  function setTab(next: typeof tab) { tab = next; localStorage.setItem('mx_admin_tab', next); }
</script>

<section class="workspace-page admin-page">
  <nav class="subnav" aria-label={`${administrationLabel} sections`}><button class:active={tab === 'accounts'} onclick={() => setTab('accounts')}>Accounts</button><button class:active={tab === 'schema'} onclick={() => setTab('schema')}>Record structure & storage</button><button class:active={tab === 'dashboard'} onclick={() => setTab('dashboard')}>{dashboardLabel}</button><button class:active={tab === 'operations'} onclick={() => setTab('operations')}>Deployment & operations</button></nav>
  {#if tab === 'accounts'}<AccountsAdmin {session} />{:else if tab === 'schema'}<SchemaAdmin />{:else if tab === 'dashboard'}<DashboardAdmin />{:else}<OperationsAdmin />{/if}
</section>
