<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { showToast } from '#lib/toast.svelte.js';
	import Icon from '#lib/components/Icon.svelte';
	import Modal from '#lib/components/Modal.svelte';

	let tokens = $state([]);
	let loaded = $state(false);
	let creating = $state(false);
	let label = $state('');
	let busy = $state(false);
	let revealed = $state(null); // { label, token } shown once

	async function load() {
		tokens = await api('/tokens');
		loaded = true;
	}
	onMount(load);

	async function create(e) {
		e.preventDefault();
		busy = true;
		try {
			const res = await api('/tokens', {
				method: 'POST',
				body: { label: label.trim() || 'App password' }
			});
			creating = false;
			label = '';
			revealed = { label: res.label, token: res.token };
			await load();
		} catch (err) {
			showToast(err.message);
		} finally {
			busy = false;
		}
	}

	async function revoke(t) {
		if (!confirm(`Revoke “${t.label}”? Apps using it will stop syncing.`)) return;
		try {
			await api('/tokens/' + t.id, { method: 'DELETE' });
			showToast('App password revoked');
			await load();
		} catch (err) {
			showToast(err.message);
		}
	}

	function copyToken() {
		navigator.clipboard?.writeText(revealed.token);
		showToast('Copied to clipboard');
	}

	function fmt(d) {
		return d ? d.replace(' ', ' · ').slice(0, 16) : 'never';
	}
</script>

<header class="spread head">
	<div>
		<h1>App passwords</h1>
		<p class="muted">
			One per device or app. CalDAV clients sign in with your username and an app password —
			never your account password.
		</p>
	</div>
	<button class="btn primary" onclick={() => (creating = true)}>
		<Icon name="plus" size={16} /> New app password
	</button>
</header>

{#if loaded && tokens.length === 0}
	<div class="card empty muted">No app passwords yet.</div>
{:else}
	<div class="list">
		{#each tokens as t (t.id)}
			<div class="card item">
				<Icon name="key" />
				<div class="meta">
					<strong>{t.label}</strong>
					<span class="muted small">created {fmt(t.created_at)} · last used {fmt(t.last_used_at)}</span>
				</div>
				<button class="btn sm danger" onclick={() => revoke(t)}>Revoke</button>
			</div>
		{/each}
	</div>
{/if}

{#if creating}
	<Modal title="New app password" onclose={() => (creating = false)}>
		<form onsubmit={create}>
			<div class="field">
				<label for="l">Label</label>
				<!-- svelte-ignore a11y_autofocus -->
				<input id="l" class="input" bind:value={label} placeholder="iPhone, Thunderbird…" autofocus />
			</div>
			<div class="spread" style="margin-top:8px">
				<button type="button" class="btn ghost" onclick={() => (creating = false)}>Cancel</button>
				<button class="btn primary" type="submit" disabled={busy}>Create</button>
			</div>
		</form>
	</Modal>
{/if}

{#if revealed}
	<Modal title="Copy your app password" onclose={() => (revealed = null)}>
		<p class="muted" style="margin-top:0">
			This is the only time you'll see <strong>{revealed.label}</strong>. Paste it into your
			CalDAV client now.
		</p>
		<div class="token">
			<code>{revealed.token}</code>
			<button class="btn sm" onclick={copyToken}><Icon name="copy" size={15} /> Copy</button>
		</div>
		<div class="spread" style="margin-top:16px">
			<span></span>
			<button class="btn primary" onclick={() => (revealed = null)}>Done</button>
		</div>
	</Modal>
{/if}

<style>
	.head {
		margin-bottom: 24px;
		align-items: flex-end;
		gap: 20px;
	}
	.head h1 {
		font-size: 24px;
	}
	.head p {
		max-width: 52ch;
	}
	.list {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px 16px;
	}
	.meta {
		display: flex;
		flex-direction: column;
		flex: 1;
	}
	.small {
		font-size: 12px;
	}
	.empty {
		padding: 30px;
		text-align: center;
	}
	.token {
		display: flex;
		align-items: center;
		gap: 10px;
		background: var(--surface-2);
		border: 1px solid var(--line);
		border-radius: 10px;
		padding: 12px 14px;
	}
	.token code {
		flex: 1;
		font-size: 17px;
		letter-spacing: 0.04em;
		word-break: break-all;
	}
</style>
