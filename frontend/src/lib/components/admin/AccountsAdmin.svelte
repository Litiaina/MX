<script lang="ts">
  import { onMount } from 'svelte';
  import type { Session } from '../../api/types';
  import type { UserSummary } from '../../api/domain';
  import { createUser, loadUsers, removeUser, resetUserPassword, resetUserSecurity, updateUserAccess } from '../../api/workspace';

  let { session }: { session: Session } = $props();
  let users = $state<UserSummary[]>([]); let loading = $state(true); let error = $state(''); let notice = $state('');
  let name = $state(''); let email = $state(''); let password = $state(''); let access = $state(2);
  let recovery = $state<{ mode: 'password' | 'security'; user: UserSummary } | null>(null);
  let newPassword = $state(''); let confirmPassword = $state(''); let adminPassword = $state(''); let factor = $state('');
  let userSearch = $state('');
  const filteredUsers = $derived(users.filter((user) => { const query = userSearch.trim().toLowerCase(); return !query || user.name.toLowerCase().includes(query) || user.email.toLowerCase().includes(query) || user.access_name.toLowerCase().includes(query); }));

  onMount(() => void refresh());
  async function refresh() { loading = true; try { users = await loadUsers(); } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function message(value: string) { notice = value; error = ''; }
  function secondFactor() { const value = factor.trim(); return /^\d{6}$/.test(value) ? { admin_otp: value, admin_recovery_code: null } : { admin_otp: null, admin_recovery_code: value || null }; }

  async function add(event: SubmitEvent) { event.preventDefault(); loading = true; try { await createUser({ name, email, password, access_level: access }); name = ''; email = ''; password = ''; access = 2; message('Account created.'); await refresh(); } catch (reason) { fail(reason); } finally { loading = false; } }
  async function saveAccess(user: UserSummary) { try { await updateUserAccess(user.uid, user.access_level); message(`Access for ${user.name} updated.`); await refresh(); } catch (reason) { fail(reason); } }
  async function remove(user: UserSummary) { if (!confirm(`Delete ${user.name}? Their account will no longer be able to sign in.`)) return; try { await removeUser(user.uid); message('Account deleted.'); await refresh(); } catch (reason) { fail(reason); } }
  function begin(mode: 'password' | 'security', user: UserSummary) { recovery = { mode, user }; newPassword = ''; confirmPassword = ''; adminPassword = ''; factor = ''; error = ''; }
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

<div class="admin-grid">
  <section class="panel"><h2>Create account</h2><p class="muted">Passwords require at least 12 characters.</p><form class="form-stack" onsubmit={add}><label>Name<input bind:value={name} required /></label><label>Email<input bind:value={email} type="email" required /></label><label>Temporary password<input bind:value={password} type="password" minlength="12" required /></label><label>Access<select bind:value={access}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select></label><button class="button primary" disabled={loading}>Create account</button></form><div class="role-guide"><p><strong>Administrator</strong> Accounts, configuration, and all record operations</p><p><strong>Manager</strong> All record operations, including deletion</p><p><strong>Editor</strong> Create, edit, upload, and download</p><p><strong>Viewer</strong> Search, read, and download only</p></div></section>
  <section class="panel admin-span"><div class="panel-heading"><div><h2>Accounts</h2><p class="muted">Access and recovery controls. Sensitive resets require your own credentials.</p></div><button class="button" onclick={refresh}>Refresh</button></div>
    {#if notice}<div class="notice success">{notice}</div>{/if}{#if error}<div class="notice error">{error}</div>{/if}
    <div class="account-tools"><input type="search" bind:value={userSearch} placeholder="Find by name, email, or access…" aria-label="Search accounts" /><span>{filteredUsers.length} of {users.length} accounts</span></div><div class="account-list">{#if loading}<div class="empty-state compact"><p>Loading accounts…</p></div>{:else if !filteredUsers.length}<div class="empty-state compact"><p>No accounts match this search.</p></div>{/if}{#each filteredUsers as user}<article class="account-row"><div><strong>{user.name}{user.uid === session.uid ? ' (you)' : ''}</strong><small>{user.email} · {user.totp_enabled ? '2FA enabled' : '2FA disabled'}</small></div><select bind:value={user.access_level} disabled={user.uid === session.uid}><option value={0}>Administrator</option><option value={1}>Manager</option><option value={2}>Editor</option><option value={3}>Viewer</option></select><div class="inline-actions"><button class="button small" disabled={user.uid === session.uid} onclick={() => saveAccess(user)}>Save access</button><button class="button small" disabled={user.uid === session.uid} onclick={() => begin('password', user)}>Reset password</button><button class="button small" disabled={user.uid === session.uid} onclick={() => begin('security', user)}>Reset 2FA</button><button class="button small danger" disabled={user.uid === session.uid} onclick={() => remove(user)}>Delete</button></div></article>{/each}</div>
  </section>
</div>

{#if recovery}<div class="overlay"><section class="dialog"><header class="dialog-head"><div><p class="eyebrow">Administrator re-authentication</p><h2>{recovery.mode === 'password' ? 'Reset password' : 'Reset authenticator'} · {recovery.user.name}</h2></div><button class="icon-button" onclick={() => recovery = null}>×</button></header><form class="form-stack" onsubmit={submitRecovery}>{#if recovery.mode === 'password'}<label>New password<input bind:value={newPassword} type="password" minlength="12" required /></label><label>Confirm password<input bind:value={confirmPassword} type="password" minlength="12" required /></label>{/if}<label>Your administrator password<input bind:value={adminPassword} type="password" required /></label><label>Your authenticator or recovery code<input bind:value={factor} placeholder="Required when your 2FA is enabled" /></label>{#if error}<div class="notice error">{error}</div>{/if}<div class="dialog-actions"><button class="button" type="button" onclick={() => recovery = null}>Cancel</button><button class="button primary" disabled={loading}>Authorize reset</button></div></form></section></div>{/if}
