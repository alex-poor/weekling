<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { showToast } from '#lib/toast.svelte.js';
	import Icon from '#lib/components/Icon.svelte';
	import Modal from '#lib/components/Modal.svelte';

	const SWATCHES = [
		'#5b8defff',
		'#2fa968ff',
		'#e0544bff',
		'#e0a63cff',
		'#9b6cf0ff',
		'#39b3c6ff',
		'#e06caeff',
		'#6b7280ff'
	];

	let calendars = $state([]);
	let shared = $state([]);
	let loaded = $state(false);
	let editing = $state(null); // null | {} (new) | calendar (existing)

	// form fields
	let fName = $state('');
	let fColor = $state(SWATCHES[0]);
	let fEvent = $state(true);
	let fTodo = $state(false);
	let busy = $state(false);

	// sharing
	let sharing = $state(null); // the calendar whose shares are open
	let shareList = $state([]);
	let users = $state([]);
	let shareGrantee = $state('');
	let shareAccess = $state('read-write');

	async function load() {
		[calendars, shared] = await Promise.all([api('/calendars'), api('/shared')]);
		loaded = true;
	}
	onMount(load);

	async function openShare(cal) {
		sharing = cal;
		shareGrantee = '';
		shareAccess = 'read-write';
		[shareList, users] = await Promise.all([
			api(`/calendars/${cal.id}/shares`),
			api('/principals')
		]);
	}

	async function addShare(e) {
		e.preventDefault();
		if (!shareGrantee) return;
		try {
			await api(`/calendars/${sharing.id}/shares`, {
				method: 'POST',
				body: { grantee: shareGrantee, access: shareAccess }
			});
			shareList = await api(`/calendars/${sharing.id}/shares`);
			shareGrantee = '';
			showToast('Shared');
		} catch (err) {
			showToast(err.message);
		}
	}

	async function removeShare(grantee) {
		try {
			await api(`/calendars/${sharing.id}/shares/${grantee}`, { method: 'DELETE' });
			shareList = await api(`/calendars/${sharing.id}/shares`);
			showToast('Access removed');
		} catch (err) {
			showToast(err.message);
		}
	}

	const ACCESS_LABEL = {
		'read-write': 'Can edit',
		read: 'Can view',
		freebusy: 'Free/busy only'
	};

	function openNew() {
		editing = {};
		fName = '';
		fColor = SWATCHES[0];
		fEvent = true;
		fTodo = false;
	}
	function openEdit(cal) {
		editing = cal;
		fName = cal.display_name;
		fColor = cal.color ?? SWATCHES[0];
		fEvent = cal.components.includes('VEVENT');
		fTodo = cal.components.includes('VTODO');
	}

	async function save(e) {
		e.preventDefault();
		if (!fName.trim()) return;
		busy = true;
		try {
			if (editing.id) {
				await api('/calendars/' + editing.id, {
					method: 'PATCH',
					body: { display_name: fName.trim(), color: fColor }
				});
				showToast('Calendar updated');
			} else {
				const comps = [fEvent && 'VEVENT', fTodo && 'VTODO'].filter(Boolean).join(',') || 'VEVENT';
				await api('/calendars', {
					method: 'POST',
					body: { display_name: fName.trim(), color: fColor, components: comps }
				});
				showToast('Calendar created');
			}
			editing = null;
			await load();
		} catch (err) {
			showToast(err.message);
		} finally {
			busy = false;
		}
	}

	async function remove(cal) {
		if (!confirm(`Delete “${cal.display_name}” and all its events? This cannot be undone.`)) return;
		try {
			await api('/calendars/' + cal.id, { method: 'DELETE' });
			showToast('Calendar deleted');
			await load();
		} catch (err) {
			showToast(err.message);
		}
	}

	function copyUrl(cal) {
		const full = location.origin + cal.url;
		navigator.clipboard?.writeText(full);
		showToast('CalDAV URL copied');
	}
</script>

<header class="spread head">
	<div>
		<h1>Calendars</h1>
		<p class="muted">Create and manage the calendars on this server.</p>
	</div>
	<button class="btn primary" onclick={openNew}><Icon name="plus" size={16} /> New calendar</button>
</header>

{#if loaded && calendars.length === 0}
	<div class="card empty">
		<p class="muted">No calendars yet — create your first one.</p>
	</div>
{:else}
	<div class="list">
		{#each calendars as cal (cal.id)}
			<div class="card item">
				<span class="dot" style="background:{cal.color ?? 'var(--accent)'}"></span>
				<div class="meta">
					<strong>{cal.display_name}</strong>
					<span class="muted small">{cal.components.join(' · ')} · {cal.url}</span>
				</div>
				<div class="acts">
					<button class="btn sm ghost" onclick={() => copyUrl(cal)} title="Copy CalDAV URL">
						<Icon name="copy" size={15} />
					</button>
					<button class="btn sm" onclick={() => openShare(cal)}>Share</button>
					<button class="btn sm" onclick={() => openEdit(cal)}>Edit</button>
					<button class="btn sm danger" onclick={() => remove(cal)} title="Delete">
						<Icon name="trash" size={15} />
					</button>
				</div>
			</div>
		{/each}
	</div>
{/if}

{#if shared.length > 0}
	<h2 class="section-title">Shared with you</h2>
	<div class="list">
		{#each shared as cal (cal.owner + '/' + cal.id)}
			<div class="card item">
				<span class="dot" style="background:{cal.color ?? 'var(--accent)'}"></span>
				<div class="meta">
					<strong>{cal.display_name}</strong>
					<span class="muted small">from @{cal.owner} · {cal.url}</span>
				</div>
				<span class="badge">{ACCESS_LABEL[cal.access] ?? cal.access}</span>
			</div>
		{/each}
	</div>
{/if}

{#if editing}
	<Modal title={editing.id ? 'Edit calendar' : 'New calendar'} onclose={() => (editing = null)}>
		<form onsubmit={save}>
			<div class="field">
				<label for="n">Name</label>
				<!-- svelte-ignore a11y_autofocus -->
				<input id="n" class="input" bind:value={fName} placeholder="Home, Work, Family…" autofocus />
			</div>

			<div class="field">
				<label>Color</label>
				<div class="swatches">
					{#each SWATCHES as c (c)}
						<button
							type="button"
							class="swatch"
							class:sel={fColor === c}
							style="background:{c}"
							aria-label={c}
							onclick={() => (fColor = c)}
						></button>
					{/each}
				</div>
			</div>

			{#if !editing.id}
				<div class="field">
					<label>Holds</label>
					<label class="check"><input type="checkbox" bind:checked={fEvent} /> Events</label>
					<label class="check"><input type="checkbox" bind:checked={fTodo} /> Tasks</label>
				</div>
			{/if}

			<div class="spread" style="margin-top:8px">
				<button type="button" class="btn ghost" onclick={() => (editing = null)}>Cancel</button>
				<button class="btn primary" type="submit" disabled={busy}>
					{editing.id ? 'Save' : 'Create'}
				</button>
			</div>
		</form>
	</Modal>
{/if}

{#if sharing}
	<Modal title={`Share “${sharing.display_name}”`} onclose={() => (sharing = null)}>
		{#if shareList.length === 0}
			<p class="muted" style="margin-top:0">Not shared with anyone yet.</p>
		{:else}
			<div class="shares">
				{#each shareList as s (s.grantee)}
					<div class="share-row">
						<div>
							<strong>{s.grantee_name}</strong>
							<span class="muted small">@{s.grantee} · {ACCESS_LABEL[s.access] ?? s.access}</span>
						</div>
						<button class="btn sm danger" onclick={() => removeShare(s.grantee)}>Remove</button>
					</div>
				{/each}
			</div>
		{/if}

		<form class="add-share" onsubmit={addShare}>
			<select class="input" bind:value={shareGrantee}>
				<option value="" disabled>Choose a person…</option>
				{#each users as u (u.id)}
					<option value={u.id}>{u.display_name} (@{u.id})</option>
				{/each}
			</select>
			<select class="input access" bind:value={shareAccess}>
				<option value="read-write">Can edit</option>
				<option value="read">Can view</option>
				<option value="freebusy">Free/busy only</option>
			</select>
			<button class="btn primary" type="submit" disabled={!shareGrantee}>Add</button>
		</form>
	</Modal>
{/if}

<style>
	.section-title {
		font-size: 15px;
		color: var(--muted);
		font-weight: 600;
		margin: 30px 0 12px;
	}
	.badge {
		font-size: 12px;
		font-weight: 600;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		padding: 4px 10px;
		border-radius: 999px;
		white-space: nowrap;
	}
	.shares {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 16px;
	}
	.share-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
	}
	.share-row > div {
		display: flex;
		flex-direction: column;
	}
	.add-share {
		display: flex;
		gap: 8px;
		border-top: 1px solid var(--line);
		padding-top: 14px;
	}
	.add-share .access {
		max-width: 160px;
	}
	.head {
		margin-bottom: 24px;
		align-items: flex-end;
	}
	.head h1 {
		font-size: 24px;
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
	.dot {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		flex: none;
	}
	.meta {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}
	.meta .small {
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.acts {
		display: flex;
		gap: 6px;
	}
	.empty {
		padding: 36px;
		text-align: center;
	}
	.swatches {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.swatch {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 2px solid transparent;
		cursor: pointer;
		padding: 0;
	}
	.swatch.sel {
		border-color: var(--fg);
		box-shadow: 0 0 0 2px var(--surface) inset;
	}
	.check {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		margin-right: 16px;
		font-weight: 500;
		color: var(--fg);
	}
</style>
