<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { showToast } from '#lib/toast.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '#lib/components/Icon.svelte';
	import Modal from '#lib/components/Modal.svelte';

	let users = $state([]);
	let loaded = $state(false);
	let denied = $state(false);

	// Add-user form
	let adding = $state(false);
	let busy = $state(false);
	let form = $state(blankForm());
	// Reset-password form
	let resetting = $state(null); // the user being reset
	let newPassword = $state('');
	// One-time app password reveal: { who, token }
	let revealed = $state(null);

	function blankForm() {
		return {
			username: '',
			display_name: '',
			email: '',
			password: '',
			is_admin: false,
			create_home: true,
			app_password: true
		};
	}

	async function load() {
		try {
			users = await api('/users');
			denied = false;
		} catch (err) {
			if (String(err.message).includes('admin')) denied = true;
			else showToast(err.message);
		} finally {
			loaded = true;
		}
	}
	onMount(load);

	async function create(e) {
		e.preventDefault();
		busy = true;
		try {
			const body = {
				username: form.username.trim(),
				display_name: form.display_name.trim() || undefined,
				email: form.email.trim() || undefined,
				password: form.password ? form.password : undefined,
				is_admin: form.is_admin,
				create_home: form.create_home,
				app_password: form.app_password
			};
			const res = await api('/users', { method: 'POST', body });
			adding = false;
			if (res.token) revealed = { who: res.display_name || res.id, token: res.token };
			else showToast(`User “${res.id}” created`);
			form = blankForm();
			await load();
		} catch (err) {
			showToast(err.message);
		} finally {
			busy = false;
		}
	}

	async function resetPassword(e) {
		e.preventDefault();
		busy = true;
		try {
			await api(`/users/${resetting.id}/password`, {
				method: 'POST',
				body: { password: newPassword }
			});
			showToast(`Password set for ${resetting.id}`);
			resetting = null;
			newPassword = '';
			await load();
		} catch (err) {
			showToast(err.message);
		} finally {
			busy = false;
		}
	}

	async function toggleAdmin(u) {
		const makeAdmin = !u.is_admin;
		const verb = makeAdmin ? 'Grant admin to' : 'Remove admin from';
		if (!confirm(`${verb} ${u.display_name} (@${u.id})?`)) return;
		try {
			await api(`/users/${u.id}/admin`, { method: 'POST', body: { is_admin: makeAdmin } });
			await load();
		} catch (err) {
			showToast(err.message);
		}
	}

	async function mintToken(u) {
		try {
			const res = await api(`/users/${u.id}/tokens`, { method: 'POST' });
			revealed = { who: u.display_name, token: res.token };
		} catch (err) {
			showToast(err.message);
		}
	}

	async function remove(u) {
		if (!confirm(`Delete ${u.display_name} (@${u.id}) and all their calendars? This cannot be undone.`))
			return;
		try {
			await api(`/users/${u.id}`, { method: 'DELETE' });
			showToast(`Deleted ${u.id}`);
			await load();
		} catch (err) {
			showToast(err.message);
		}
	}

	function copyToken() {
		navigator.clipboard?.writeText(revealed.token);
		showToast('Copied to clipboard');
	}

	const me = $derived(session.user?.id);
</script>

<header class="spread head">
	<div>
		<h1>Users<span class="dot">.</span></h1>
		<p class="muted">
			Each family member gets their own account. Create one here, then share a calendar with them
			from the Calendars page.
		</p>
	</div>
	<button class="btn primary" onclick={() => (adding = true)}>
		<Icon name="plus" size={16} /> Add user
	</button>
</header>

{#if denied}
	<div class="card empty muted">You need to be an admin to manage users.</div>
{:else if loaded}
	<div class="list">
		{#each users as u (u.id)}
			<div class="card item">
				<div class="avatar">{(u.display_name ?? '?').slice(0, 1)}</div>
				<div class="meta">
					<strong>{u.display_name} <span class="handle">@{u.id}</span></strong>
					<span class="muted small">
						{u.email || 'no email'}
						{#if u.is_admin}· <span class="badge admin">admin</span>{/if}
						{#if !u.has_password}· <span class="badge">no web login</span>{/if}
					</span>
				</div>
				<div class="actions">
					<button class="btn sm ghost" onclick={() => mintToken(u)} title="New CalDAV app password">
						<Icon name="key" size={15} /> App pw
					</button>
					<button class="btn sm ghost" onclick={() => (resetting = u)}>Set password</button>
					<button class="btn sm ghost" onclick={() => toggleAdmin(u)}>
						{u.is_admin ? 'Remove admin' : 'Make admin'}
					</button>
					<button
						class="btn sm danger"
						onclick={() => remove(u)}
						disabled={u.id === me}
						title={u.id === me ? "You can't delete your own account" : 'Delete user'}
					>
						<Icon name="trash" size={15} />
					</button>
				</div>
			</div>
		{/each}
	</div>
{/if}

{#if adding}
	<Modal title="Add a user" onclose={() => (adding = false)}>
		<form onsubmit={create}>
			<div class="field">
				<label for="u">Username</label>
				<!-- svelte-ignore a11y_autofocus -->
				<input id="u" class="input" bind:value={form.username} placeholder="sam" autofocus />
			</div>
			<div class="field">
				<label for="d">Display name</label>
				<input id="d" class="input" bind:value={form.display_name} placeholder="Sam" />
			</div>
			<div class="field">
				<label for="e">Email (optional)</label>
				<input id="e" class="input" bind:value={form.email} placeholder="sam@example.com" />
			</div>
			<div class="field">
				<label for="p">Web-UI password (optional)</label>
				<input
					id="p"
					class="input"
					type="password"
					bind:value={form.password}
					placeholder="at least 8 characters"
				/>
			</div>
			<label class="check"><input type="checkbox" bind:checked={form.is_admin} /> Administrator</label>
			<label class="check">
				<input type="checkbox" bind:checked={form.create_home} /> Create a starter “Home” calendar
			</label>
			<label class="check">
				<input type="checkbox" bind:checked={form.app_password} /> Generate a CalDAV app password now
			</label>
			<div class="spread" style="margin-top:12px">
				<button type="button" class="btn ghost" onclick={() => (adding = false)}>Cancel</button>
				<button class="btn primary" type="submit" disabled={busy}>Create user</button>
			</div>
		</form>
	</Modal>
{/if}

{#if resetting}
	<Modal title={`Set password for ${resetting.id}`} onclose={() => (resetting = null)}>
		<form onsubmit={resetPassword}>
			<div class="field">
				<label for="np">New web-UI password</label>
				<!-- svelte-ignore a11y_autofocus -->
				<input
					id="np"
					class="input"
					type="password"
					bind:value={newPassword}
					placeholder="at least 8 characters"
					autofocus
				/>
			</div>
			<div class="spread" style="margin-top:12px">
				<button type="button" class="btn ghost" onclick={() => (resetting = null)}>Cancel</button>
				<button class="btn primary" type="submit" disabled={busy}>Set password</button>
			</div>
		</form>
	</Modal>
{/if}

{#if revealed}
	<Modal title="Copy the app password" onclose={() => (revealed = null)}>
		<p class="muted" style="margin-top:0">
			This is the only time you'll see the app password for <strong>{revealed.who}</strong>. Paste
			it into their CalDAV client now.
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
		font-size: clamp(34px, 6vw, 52px);
	}
	.head p {
		max-width: 54ch;
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
	.avatar {
		width: 42px;
		height: 42px;
		flex: none;
		border-radius: 50%;
		background: var(--green);
		color: #fff;
		display: grid;
		place-items: center;
		font-family: var(--font-display);
		font-weight: 800;
		font-size: 17px;
		box-shadow: 0 0 0 3px #fff;
		text-transform: uppercase;
	}
	.meta {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}
	.handle {
		font-weight: 500;
		color: rgba(17, 17, 17, 0.5);
	}
	.small {
		font-size: 12px;
	}
	.badge {
		display: inline-block;
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 11px;
		padding: 1px 8px;
		border-radius: 999px;
		background: rgba(17, 17, 17, 0.08);
	}
	.badge.admin {
		background: var(--blue);
		color: #fff;
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
	.check {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 14px;
		margin-top: 8px;
	}
	.check input {
		width: 16px;
		height: 16px;
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
	@media (max-width: 620px) {
		.item {
			flex-wrap: wrap;
		}
		.actions {
			flex-basis: 100%;
			justify-content: flex-start;
		}
	}
</style>
