<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { showToast } from '#lib/toast.svelte.js';
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

	// Black or white text for a given slab colour (hex, optionally 8-digit).
	function textOn(hex) {
		const h = (hex ?? '#3b5bff').replace('#', '').slice(0, 6);
		const r = parseInt(h.slice(0, 2), 16) || 0;
		const g = parseInt(h.slice(2, 4), 16) || 0;
		const b = parseInt(h.slice(4, 6), 16) || 0;
		const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
		return lum > 0.62 ? '#111' : '#fff';
	}
	const BADGE = { VEVENT: 'var(--yellow)', VTODO: 'var(--green)' };
</script>

<header class="head">
	<div>
		<h1>Calendars<span class="dot">.</span></h1>
		<p class="muted">Create and manage the calendars on this server.</p>
	</div>
	<button class="btn primary big" onclick={openNew}>+ New calendar</button>
</header>

{#if loaded && calendars.length === 0}
	<div class="empty">
		<img src="/logo.png?v=2" alt="" class="mascot-sm" />
		<p>No calendars yet — even a weekling starts with one rep.</p>
		<button class="btn primary" onclick={openNew}>+ New calendar</button>
	</div>
{:else}
	<div class="slabs">
		{#each calendars as cal (cal.id)}
			<div class="slab" style="background:{cal.color ?? 'var(--blue)'}; color:{textOn(cal.color)}">
				<div class="badges">
					{#each cal.components as c (c)}
						<span class="sticker" style="background:{BADGE[c] ?? 'var(--yellow)'}">{c}</span>
					{/each}
				</div>
				<h2 class="slab-name">{cal.display_name}</h2>
				<div class="path">
					<code>{cal.url}</code>
					<button class="link-btn" onclick={() => copyUrl(cal)}>Copy</button>
				</div>
				<div class="slab-acts">
					<button class="chip solid" onclick={() => openShare(cal)}>Share</button>
					<button class="chip outline" onclick={() => openEdit(cal)}>Edit</button>
					<button class="link-btn" onclick={() => remove(cal)}>Delete</button>
				</div>
			</div>
		{/each}
	</div>
{/if}

{#if shared.length > 0}
	<h2 class="section-title">Shared with you</h2>
	<div class="slabs">
		{#each shared as cal (cal.owner + '/' + cal.id)}
			<div class="slab small" style="background:{cal.color ?? 'var(--blue)'}; color:{textOn(cal.color)}">
				<div class="badges">
					<span class="sticker" style="background:#fff">{ACCESS_LABEL[cal.access] ?? cal.access}</span>
				</div>
				<h2 class="slab-name">{cal.display_name}</h2>
				<div class="path"><code>from @{cal.owner}</code></div>
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
		margin-bottom: 28px;
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
	}
	.head h1 {
		font-size: clamp(42px, 7vw, 66px);
	}
	.head p {
		margin: 8px 0 0;
	}
	.btn.big {
		height: 48px;
		font-size: 16px;
		padding: 0 24px;
	}

	.slabs {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
		gap: 22px;
	}
	.slab {
		position: relative;
		border-radius: 24px;
		padding: 24px;
		box-shadow: var(--shadow);
		min-height: 186px;
		display: flex;
		flex-direction: column;
	}
	.slab.small {
		min-height: 128px;
	}
	.badges {
		position: absolute;
		top: -12px;
		right: 18px;
		display: flex;
		gap: 8px;
	}
	.badges .sticker:nth-child(2) {
		transform: rotate(5deg);
	}
	.slab-name {
		font-size: clamp(26px, 4vw, 32px);
		color: inherit;
		margin: 2px 0 auto;
		overflow-wrap: anywhere;
	}
	.path {
		display: flex;
		align-items: center;
		gap: 10px;
		background: rgba(0, 0, 0, 0.15);
		border-radius: 13px;
		padding: 10px 13px;
		margin: 16px 0;
		min-width: 0;
	}
	.path code {
		font-family: var(--font-mono);
		font-size: 12.5px;
		color: inherit;
		opacity: 0.95;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}
	.slab-acts {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.chip {
		height: 36px;
		padding: 0 17px;
		border-radius: 999px;
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 14px;
		cursor: pointer;
	}
	.chip.solid {
		background: #fff;
		color: #111;
		border: none;
		box-shadow: var(--shadow-sm);
	}
	.chip.outline {
		background: transparent;
		color: inherit;
		border: 2px solid currentColor;
	}
	.link-btn {
		background: none;
		border: none;
		color: inherit;
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 14px;
		cursor: pointer;
		text-decoration: underline;
		text-decoration-thickness: 2px;
		text-underline-offset: 2px;
		padding: 4px;
	}
	.slab-acts .link-btn {
		margin-left: auto;
	}
	.empty {
		padding: 46px 30px;
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
	}
	.mascot-sm {
		width: 118px;
		height: auto;
		transform: rotate(-4deg);
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
