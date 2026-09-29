<script lang="ts">
  import { profilePhotoUrl } from '../util/profilePhoto';

  let {
    userUid,
    name,
    updatedAt = null,
    online = false
  }: {
    userUid: string;
    name: string;
    updatedAt?: number | null;
    online?: boolean;
  } = $props();

  let source = $state('');

  $effect(() => {
    const version = updatedAt;
    source = '';
    if (!version) return;
    let current = true;
    void profilePhotoUrl(userUid, version)
      .then((url) => { if (current) source = url; })
      .catch(() => { if (current) source = ''; });
    return () => { current = false; };
  });

  function initials(value: string) {
    return value.split(/\s+/).filter(Boolean).map((part) => part[0]).join('').slice(0, 2).toUpperCase() || 'MX';
  }
</script>

<span class:with-photo={!!source} class="profile-avatar-image" aria-hidden="true">
  {#if source}<img src={source} alt="" decoding="async" />{:else}<span>{initials(name)}</span>{/if}
  {#if online}<i class="profile-avatar-presence"></i>{/if}
</span>
