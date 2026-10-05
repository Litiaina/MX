<script lang="ts">
  import KeyRound from '@lucide/svelte/icons/key-round';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import ShieldOff from '@lucide/svelte/icons/shield-off';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UserCog from '@lucide/svelte/icons/user-cog';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import Boxes from '@lucide/svelte/icons/boxes';
  import Save from '@lucide/svelte/icons/save';
  import X from '@lucide/svelte/icons/x';
  import { onMount } from 'svelte';
  import type { Session } from '../../api/types';
  import type { AccountModuleAccessResponse, AccountModuleGrant, UserSummary } from '../../api/domain';
  import { requestConfirmation } from '../../confirmation';
  import { createUser, loadAccountModuleAccess, loadUsers, removeUser, resetUserPassword, resetUserSecurity, saveAccountModuleAccess, updateUserAccess } from '../../api/workspace';
  import ProfileAvatar from '../ProfileAvatar.svelte';

  let { session }: { session: Session } = $props();
  let users = $state<UserSummary[]>([]); let loading = $state(true); let error = $state(''); let notice = $state('');
  let name = $state(''); let email = $state(''); let password = $state(''); let access = $state(2);
  let recovery = $state<{ mode: 'password' | 'security'; user: UserSummary } | null>(null);
  let createOpen = $state(false);
  let managedUser = $state<UserSummary | null>(null);
  let newPassword = $state(''); let confirmPassword = $state(''); let adminPassword = $state(''); let factor = $state('');
  let userSearch = $state('');
  let moduleAccess = $state<AccountModuleAccessResponse | null>(null);
  let moduleAccessLoading = $state(false);
  let moduleAccessSaving = $state(false);
  let moduleAccessNotice = $state('');
  const filteredUsers = $derived(users.filter((user) => { const query = userSearch.trim().toLowerCase(); return !query || user.name.toLowerCase().includes(query) || user.email.toLowerCase().includes(query) || user.access_name.toLowerCase().includes(query); }));
  const securedAccounts = $derived(users.filter((user) => user.totp_enabled).length);
  const administratorAccounts = $derived(users.filter((user) => user.access_level === 0).length);
  type ModuleCapability = 'can_read' | 'can_create' | 'can_update' | 'can_delete' | 'can_configure' | 'can_report' | 'can_attachments';

  onMount(() => void refresh());
  async function refresh() { loading = true; try { users = await loadUsers(); } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; moduleAccessNotice = ''; }
  function message(value: string) { notice = value; error = ''; }
  function secondFactor() { const value = factor.trim(); return /^\d{6}$/.test(value) ? { admin_otp: value, admin_recovery_code: null } : { admin_otp: null, admin_recovery_code: value || null }; }

  async function add(event: SubmitEvent) { event.preventDefault(); loading = true; try { await createUser({ name, email, password, access_level: access }); name = ''; email = ''; password = ''; access = 2; createOpen = false; message('Account created.'); await refresh(); } catch (reason) { fail(reason); } finally { loading = false; } }
  async function saveAccess(user: UserSummary) { try { await updateUserAccess(user.uid, user.access_level); managedUser = null; message(`Access for ${user.name} updated.`); await refresh(); } catch (reason) { fail(reason); } }
  async function openManagement(user: UserSummary) {
    managedUser = user;
    moduleAccess = null;
    moduleAccessNotice = '';
    error = '';
    if (user.access_level === 0) return;
    moduleAccessLoading = true;
    try { moduleAccess = await loadAccountModuleAccess(user.uid); }
    catch (reason) { fail(reason); }
    finally { moduleAccessLoading = false; }
  }
  function setModuleGrant(moduleUid: string, key: ModuleCapability, checked: boolean) {
    if (!moduleAccess) return;
    moduleAccessNotice = '';
    moduleAccess = { ...moduleAccess, modules: moduleAccess.modules.map((item) => item.module_uid === moduleUid ? { ...item, grant: { ...item.grant, [key]: checked } } : item) };
  }
  function setModuleEnabled(moduleUid: string, enabled: boolean) {
    if (!moduleAccess) return;
    moduleAccessNotice = '';
    moduleAccess = { ...moduleAccess, modules: moduleAccess.modules.map((item) => item.module_uid === moduleUid ? {
      ...item,
      grant: enabled ? {
        module_uid: item.module_uid,
        can_read: item.role_permission.can_read,
        can_create: item.role_permission.can_create,
        can_update: item.role_permission.can_update,
        can_delete: item.role_permission.can_delete,
        can_configure: item.role_permission.can_configure,
        can_report: item.role_permission.can_report,
        can_attachments: item.role_permission.can_attachments
      } : { module_uid: item.module_uid, can_read: false, can_create: false, can_update: false, can_delete: false, can_configure: false, can_report: false, can_attachments: false }
    } : item) };
  }
  async function saveModuleGrants() {
    if (!managedUser || !moduleAccess) return;
    moduleAccessSaving = true; error = ''; moduleAccessNotice = '';
    try {
      const result = await saveAccountModuleAccess(managedUser.uid, moduleAccess.revision, moduleAccess.modules.map((item) => item.grant));
      moduleAccess = { ...moduleAccess, revision: result.revision };
      moduleAccessNotice = `Module access for ${managedUser.name} saved.`;
    } catch (reason) { fail(reason); }
    finally { moduleAccessSaving = false; }
  }
  async function remove(user: UserSummary) { if (!await requestConfirmation({ title: `Delete ${user.name}?`, description: 'Their account will no longer be able to sign in. This is an administrator action.', confirmLabel: 'Delete account' })) return; try { await removeUser(user.uid); managedUser = null; message('Account deleted.'); await refresh(); } catch (reason) { fail(reason); } }
  function begin(mode: 'password' | 'security', user: UserSummary) { managedUser = null; recovery = { mode, user }; newPassword = ''; confirmPassword = ''; adminPassword = ''; factor = ''; error = ''; }
  function roleName(level: number) { return ['Administrator', 'Manager', 'Editor', 'Viewer'][level] || `Access level ${level}`; }
  async function submitRecovery(event: SubmitEvent) {
    event.preventDefault(); if (!recovery) return;
    if (recovery.mode === 'password' && newPassword !== confirmPassword) { error = 'New passwords do not match.'; return; }
    loading = true;
    try {
      const auth = { admin_password: adminPassword, ...secondFactor() };
      if (recovery.mode === 'password') await resetUserPassword(recovery.user.uid, { ...auth, new_password: newPassword });
      else await resetUserSecurity(recovery.user.uid, auth);
      message(recovery.mode === 'password' ? 'Password reset and sessions revoked.' : 'Authenticator reset and sessions revoked.'); recovery = null; await refresh();
    } catch (reason) { fail(reason); } finally { loading = false; }
  }
</script>

<section class="panel accounts-workspace">
  <header class="admin-section-heading">
    <div class="admin-section-title"><span><UsersRound size={20} /></span><div><h2>Accounts</h2><p>Manage access and account recovery without exposing sensitive controls in the directory.</p></div></div>
    <div class="inline-actions"><button class="button icon-labelled" onclick={refresh} disabled={loading}><RefreshCw size={16} /><span>Refresh</span></button><button class="button primary icon-labelled" onclick={() => { createOpen = true; error = ''; }}><UserPlus size={16} /><span>Create account</span></button></div>
  </header>

  {#if notice}<div class="notice success">{notice}</div>{/if}{#if error && !createOpen && !managedUser && !recovery}<div class="notice error">{error}</div>{/if}

  <div class="account-summary" aria-label="Account summary">
    <article><UsersRound size={18} /><span><strong>{users.length}</strong><small>Total accounts</small></span></article>
    <article><ShieldCheck size={18} /><span><strong>{securedAccounts}</strong><small>Protected by 2FA</small></span></article>
    <article><UserCog size={18} /><span><strong>{administratorAccounts}</strong><small>Administrators</small></span></article>
  </div>

  <div class="account-directory-tools">
    <label class="account-search"><Search size={16} /><input type="search" bind:value={userSearch} placeholder="Search name, email, or role…" aria-label="Search accounts" /></label>
    <span>{filteredUsers.length === users.length ? `${users.length} account${users.length === 1 ? '' : 's'}` : `${filteredUsers.length} of ${users.length} accounts`}</span>
  </div>

  <div class="account-directory">
    {#if loading}<div class="empty-state compact"><p>Loading accounts…</p></div>{:else if !filteredUsers.length}<div class="empty-state compact"><p>No accounts match this search.</p></div>{/if}
    {#each filteredUsers as user}
      <article class="account-card">
        <span class="account-avatar" aria-hidden="true"><ProfileAvatar userUid={user.uid} name={user.name} updatedAt={user.profile_photo_updated_at} /></span>
        <div class="account-identity"><div><strong>{user.name}</strong>{#if user.uid === session.uid}<span class="account-you">You</span>{/if}</div><small>{user.email}</small></div>
        <div class:enabled={user.totp_enabled} class="account-security"><span>{#if user.totp_enabled}<ShieldCheck size={15} />{:else}<ShieldOff size={15} />{/if}</span><div><small>Security</small><strong>{user.totp_enabled ? '2FA enabled' : '2FA disabled'}</strong></div></div>
        <div class="account-role"><small>Access</small><strong>{roleName(user.access_level)}</strong></div>
        {#if user.uid === session.uid}<button class="button small" onclick={() => location.hash = '#account'}>My account</button>{:else}<button class="button small icon-labelled" onclick={() => void openManagement(user)}><UserCog size={15} /><span>Manage</span></button>{/if}
      </article>
    {/each}
  </div>
</section>

{#if createOpen}<div class="overlay"><div class="dialog account-admin-dialog" role="dialog" aria-modal="true" aria-labelledby="create-account-title"><header class="dialog-head"><div class="admin-dialog-title"><span><UserPlus size={19} /></span><div><p class="eyebrow">Identity and access</p><h2 id="create-account-title">Create account</h2><p>Set the person’s initial access. They can personalize their account after signing in.</p></div></div><button class="icon-button" aria-label="Close account creation" onclick={() => createOpen = false}><X size={18} /></button></header><form class="form-stack account-create-form" onsubmit={add}><label>Name<input bind:value={name} autocomplete="off" required /></label><label>Email<input bind:value={email} type="email" autocomplete="off" required /></label><label class="full">Temporary password<input bind:value={password} type="password" minlength="12" autocomplete="new-password" required /><small>Use at least 12 characters.</small></label><label class="full">Access<select bind:value={access}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select></label><details class="role-guide full"><summary>What can each role do?</summary><p><strong>Administrator</strong> Accounts, configuration, and all record operations</p><p><strong>Manager</strong> All record operations, including deletion</p><p><strong>Editor</strong> Create, edit, upload, and download</p><p><strong>Viewer</strong> Search, read, and download only</p></details>{#if error}<div class="notice error full">{error}</div>{/if}<div class="dialog-actions"><button class="button" type="button" onclick={() => createOpen = false}>Cancel</button><button class="button primary icon-labelled" disabled={loading}><UserPlus size={16} /><span>Create account</span></button></div></form></div></div>{/if}

{#if managedUser}<div class="overlay"><div class="dialog account-admin-dialog account-access-dialog" role="dialog" aria-modal="true" aria-labelledby="manage-account-title">
  <header class="dialog-head"><div class="admin-dialog-title"><span class="account-avatar"><ProfileAvatar userUid={managedUser.uid} name={managedUser.name} updatedAt={managedUser.profile_photo_updated_at} /></span><div><p class="eyebrow">Identity and access</p><h2 id="manage-account-title">{managedUser.name}</h2><p>{managedUser.email}</p></div></div><button class="icon-button" aria-label="Close account management" onclick={() => { managedUser = null; moduleAccess = null; }}><X size={18} /></button></header>
  <div class="account-management">
    <section><h3>System role</h3><p>The role sets the maximum permissions this account can receive. Module grants below can only reduce that ceiling.</p><label>Role<select bind:value={managedUser.access_level}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select></label><button class="button primary icon-labelled" onclick={() => saveAccess(managedUser!)} disabled={loading}><ShieldCheck size={16} /><span>Save role</span></button></section>

    <section class="account-module-access"><div class="account-access-heading"><span><Boxes size={18} /></span><div><h3>Module access</h3><p>Default deny: modules without an explicit grant are hidden and rejected by the API.</p></div>{#if moduleAccess}<small>Revision {moduleAccess.revision}</small>{/if}</div>
      {#if managedUser.access_level === 0}<div class="account-access-admin"><ShieldCheck size={18} /><span><strong>Full system access</strong><small>Administrators always have every capability in active modules.</small></span></div>
      {:else if moduleAccessLoading}<div class="empty-state compact"><p>Loading module grants…</p></div>
      {:else if moduleAccess}
        <div class="account-module-list">
          {#each moduleAccess.modules as item}
            {@const enabled = item.grant.can_read || item.grant.can_configure}
            <article class:disabled={!enabled}>
              <header><span class="module-access-icon" style={`--module-color:${item.module_color}`}><Boxes size={15} /></span><span><strong>{item.module_name}</strong><small>{item.module_active ? (enabled ? 'Visible to this account' : 'Hidden · no grant') : 'Archived module'}</small></span><label class="access-switch"><input type="checkbox" checked={enabled} disabled={!item.module_active} onchange={(event) => setModuleEnabled(item.module_uid, event.currentTarget.checked)} aria-label={`Allow ${managedUser!.name} to access ${item.module_name}`} /><span>Access</span></label></header>
              {#if enabled}<div class="module-capability-grid">
                {#each [
                  ['can_read','View'],['can_create','Create'],['can_update','Edit'],['can_delete','Archive'],
                  ['can_configure','Configure'],['can_report','Reports'],['can_attachments','Files']
                ] as capability}
                  {@const key = capability[0] as ModuleCapability}
                  <label title={!item.role_permission[key] ? `The ${roleName(managedUser!.access_level)} role does not allow this capability.` : ''}><input type="checkbox" checked={item.grant[key]} disabled={!item.role_permission[key] || key === 'can_read'} onchange={(event) => setModuleGrant(item.module_uid, key, event.currentTarget.checked)} /><span>{capability[1]}</span></label>
                {/each}
              </div>{/if}
            </article>
          {/each}
        </div>
        {#if moduleAccessNotice}<div class="notice success">{moduleAccessNotice}</div>{/if}
        <div class="account-access-save"><span>Saving uses revision {moduleAccess.revision}; concurrent administrator changes are rejected for review.</span><button class="button primary icon-labelled" onclick={() => void saveModuleGrants()} disabled={moduleAccessSaving}><Save size={15} /><span>{moduleAccessSaving ? 'Saving…' : 'Save module access'}</span></button></div>
      {/if}
    </section>

    <section><h3>Security recovery</h3><p>These actions revoke the account’s active sessions and require your administrator credentials.</p><div class="account-recovery-actions"><button class="button icon-labelled" onclick={() => begin('password', managedUser!)}><KeyRound size={16} /><span>Reset password</span></button><button class="button icon-labelled" onclick={() => begin('security', managedUser!)}><ShieldOff size={16} /><span>Reset 2FA</span></button></div></section>
    <section class="account-danger-zone"><div><h3>Delete account</h3><p>Permanently prevent this account from signing in.</p></div><button class="button danger icon-labelled" onclick={() => remove(managedUser!)}><Trash2 size={16} /><span>Delete account</span></button></section>
  </div>{#if error}<div class="notice error">{error}</div>{/if}
</div></div>{/if}

{#if recovery}<div class="overlay"><section class="dialog account-admin-dialog"><header class="dialog-head"><div class="admin-dialog-title"><span><KeyRound size={19} /></span><div><p class="eyebrow">Administrator re-authentication</p><h2>{recovery.mode === 'password' ? 'Reset password' : 'Reset authenticator'}</h2><p>{recovery.user.name} · {recovery.user.email}</p></div></div><button class="icon-button" aria-label="Close" onclick={() => recovery = null}><X size={18} /></button></header><form class="form-stack" onsubmit={submitRecovery}>{#if recovery.mode === 'password'}<label>New password<input bind:value={newPassword} type="password" minlength="12" required /></label><label>Confirm password<input bind:value={confirmPassword} type="password" minlength="12" required /></label>{/if}<label>Your administrator password<input bind:value={adminPassword} type="password" required /></label><label>Your authenticator or recovery code<input bind:value={factor} placeholder="Required when your 2FA is enabled" /></label>{#if error}<div class="notice error">{error}</div>{/if}<div class="dialog-actions"><button class="button" type="button" onclick={() => recovery = null}>Cancel</button><button class="button primary" disabled={loading}>Authorize reset</button></div></form></section></div>{/if}
