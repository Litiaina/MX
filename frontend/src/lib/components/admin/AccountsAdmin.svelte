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
  import X from '@lucide/svelte/icons/x';
  import { onMount } from 'svelte';
  import type { Session } from '../../api/types';
  import type { UserSummary } from '../../api/domain';
  import { requestConfirmation } from '../../confirmation';
  import { createUser, loadUsers, removeUser, resetUserPassword, resetUserSecurity, updateUserAccess } from '../../api/workspace';
  import ProfileAvatar from '../ProfileAvatar.svelte';

  let { session }: { session: Session } = $props();
  let users = $state<UserSummary[]>([]); let loading = $state(true); let error = $state(''); let notice = $state('');
  let name = $state(''); let email = $state(''); let password = $state(''); let access = $state(2);
  let recovery = $state<{ mode: 'password' | 'security'; user: UserSummary } | null>(null);
  let createOpen = $state(false);
  let managedUser = $state<UserSummary | null>(null);
  let newPassword = $state(''); let confirmPassword = $state(''); let adminPassword = $state(''); let factor = $state('');
  let userSearch = $state('');
  const filteredUsers = $derived(users.filter((user) => { const query = userSearch.trim().toLowerCase(); return !query || user.name.toLowerCase().includes(query) || user.email.toLowerCase().includes(query) || user.access_name.toLowerCase().includes(query); }));
  const securedAccounts = $derived(users.filter((user) => user.totp_enabled).length);
  const administratorAccounts = $derived(users.filter((user) => user.access_level === 0).length);

  onMount(() => void refresh());
  async function refresh() { loading = true; try { users = await loadUsers(); } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function message(value: string) { notice = value; error = ''; }
  function secondFactor() { const value = factor.trim(); return /^\d{6}$/.test(value) ? { admin_otp: value, admin_recovery_code: null } : { admin_otp: null, admin_recovery_code: value || null }; }

  async function add(event: SubmitEvent) { event.preventDefault(); loading = true; try { await createUser({ name, email, password, access_level: access }); name = ''; email = ''; password = ''; access = 2; createOpen = false; message('Account created.'); await refresh(); } catch (reason) { fail(reason); } finally { loading = false; } }
  async function saveAccess(user: UserSummary) { try { await updateUserAccess(user.uid, user.access_level); managedUser = null; message(`Access for ${user.name} updated.`); await refresh(); } catch (reason) { fail(reason); } }
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
        {#if user.uid === session.uid}<button class="button small" onclick={() => location.hash = '#account'}>My account</button>{:else}<button class="button small icon-labelled" onclick={() => { managedUser = user; error = ''; }}><UserCog size={15} /><span>Manage</span></button>{/if}
      </article>
    {/each}
  </div>
</section>

{#if createOpen}<div class="overlay"><div class="dialog account-admin-dialog" role="dialog" aria-modal="true" aria-labelledby="create-account-title"><header class="dialog-head"><div class="admin-dialog-title"><span><UserPlus size={19} /></span><div><p class="eyebrow">Identity and access</p><h2 id="create-account-title">Create account</h2><p>Set the person’s initial access. They can personalize their account after signing in.</p></div></div><button class="icon-button" aria-label="Close account creation" onclick={() => createOpen = false}><X size={18} /></button></header><form class="form-stack account-create-form" onsubmit={add}><label>Name<input bind:value={name} autocomplete="off" required /></label><label>Email<input bind:value={email} type="email" autocomplete="off" required /></label><label class="full">Temporary password<input bind:value={password} type="password" minlength="12" autocomplete="new-password" required /><small>Use at least 12 characters.</small></label><label class="full">Access<select bind:value={access}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select></label><details class="role-guide full"><summary>What can each role do?</summary><p><strong>Administrator</strong> Accounts, configuration, and all record operations</p><p><strong>Manager</strong> All record operations, including deletion</p><p><strong>Editor</strong> Create, edit, upload, and download</p><p><strong>Viewer</strong> Search, read, and download only</p></details>{#if error}<div class="notice error full">{error}</div>{/if}<div class="dialog-actions"><button class="button" type="button" onclick={() => createOpen = false}>Cancel</button><button class="button primary icon-labelled" disabled={loading}><UserPlus size={16} /><span>Create account</span></button></div></form></div></div>{/if}

{#if managedUser}<div class="overlay"><div class="dialog account-admin-dialog" role="dialog" aria-modal="true" aria-labelledby="manage-account-title"><header class="dialog-head"><div class="admin-dialog-title"><span class="account-avatar"><ProfileAvatar userUid={managedUser.uid} name={managedUser.name} updatedAt={managedUser.profile_photo_updated_at} /></span><div><p class="eyebrow">Account controls</p><h2 id="manage-account-title">{managedUser.name}</h2><p>{managedUser.email}</p></div></div><button class="icon-button" aria-label="Close account management" onclick={() => managedUser = null}><X size={18} /></button></header><div class="account-management"><section><h3>Access level</h3><p>Controls which MX workspaces and operations this account can use.</p><label>Role<select bind:value={managedUser.access_level}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select></label><button class="button primary icon-labelled" onclick={() => saveAccess(managedUser!)} disabled={loading}><ShieldCheck size={16} /><span>Save access</span></button></section><section><h3>Security recovery</h3><p>These actions revoke the account’s active sessions and require your administrator credentials.</p><div class="account-recovery-actions"><button class="button icon-labelled" onclick={() => begin('password', managedUser!)}><KeyRound size={16} /><span>Reset password</span></button><button class="button icon-labelled" onclick={() => begin('security', managedUser!)}><ShieldOff size={16} /><span>Reset 2FA</span></button></div></section><section class="account-danger-zone"><div><h3>Delete account</h3><p>Permanently prevent this account from signing in.</p></div><button class="button danger icon-labelled" onclick={() => remove(managedUser!)}><Trash2 size={16} /><span>Delete account</span></button></section></div>{#if error}<div class="notice error">{error}</div>{/if}</div></div>{/if}

{#if recovery}<div class="overlay"><section class="dialog account-admin-dialog"><header class="dialog-head"><div class="admin-dialog-title"><span><KeyRound size={19} /></span><div><p class="eyebrow">Administrator re-authentication</p><h2>{recovery.mode === 'password' ? 'Reset password' : 'Reset authenticator'}</h2><p>{recovery.user.name} · {recovery.user.email}</p></div></div><button class="icon-button" aria-label="Close" onclick={() => recovery = null}><X size={18} /></button></header><form class="form-stack" onsubmit={submitRecovery}>{#if recovery.mode === 'password'}<label>New password<input bind:value={newPassword} type="password" minlength="12" required /></label><label>Confirm password<input bind:value={confirmPassword} type="password" minlength="12" required /></label>{/if}<label>Your administrator password<input bind:value={adminPassword} type="password" required /></label><label>Your authenticator or recovery code<input bind:value={factor} placeholder="Required when your 2FA is enabled" /></label>{#if error}<div class="notice error">{error}</div>{/if}<div class="dialog-actions"><button class="button" type="button" onclick={() => recovery = null}>Cancel</button><button class="button primary" disabled={loading}>Authorize reset</button></div></form></section></div>{/if}
