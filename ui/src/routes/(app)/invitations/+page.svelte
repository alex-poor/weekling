<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { showToast } from '#lib/toast.svelte.js';
	import Icon from '#lib/components/Icon.svelte';

	let items = $state([]);
	let loaded = $state(false);

	async function load() {
		items = await api('/inbox');
		loaded = true;
	}
	onMount(load);

	async function respond(item, partstat) {
		try {
			await api(`/inbox/${item.id}/reply`, { method: 'POST', body: { partstat } });
			showToast(partstat === 'ACCEPTED' ? 'Accepted' : partstat === 'DECLINED' ? 'Declined' : 'Marked tentative');
			await load();
		} catch (err) {
			showToast(err.message);
		}
	}

	function fmtDate(dt) {
		// dt is 'YYYYMMDDTHHMMSS'
		if (!dt || dt.length < 15) return '';
		const d = new Date(
			`${dt.slice(0, 4)}-${dt.slice(4, 6)}-${dt.slice(6, 8)}T${dt.slice(9, 11)}:${dt.slice(11, 13)}:00Z`
		);
		return d.toLocaleString(undefined, {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			hour: 'numeric',
			minute: '2-digit'
		});
	}

	const METHOD_LABEL = { REQUEST: 'Invitation', CANCEL: 'Cancelled', REPLY: 'Reply' };
</script>

<header class="head">
	<h1>Invitations<span class="dot">.</span></h1>
	<p class="muted">Scheduling messages from other people on this server.</p>
</header>

{#if loaded && items.length === 0}
	<div class="card empty muted">
		<Icon name="calendar" size={22} />
		<p>No pending invitations.</p>
	</div>
{:else}
	<div class="list">
		{#each items as item (item.id)}
			<div class="card invite">
				<div class="info">
					<div class="row1">
						<span class="method {item.method?.toLowerCase()}">
							{METHOD_LABEL[item.method] ?? item.method}
						</span>
						<strong>{item.summary ?? '(no title)'}</strong>
					</div>
					<span class="muted small">
						{fmtDate(item.dtstart)}{item.organizer ? ` · from ${item.organizer}` : ''}
					</span>
				</div>
				{#if item.method === 'REQUEST'}
					<div class="acts">
						<button class="btn sm primary" onclick={() => respond(item, 'ACCEPTED')}>Accept</button>
						<button class="btn sm" onclick={() => respond(item, 'TENTATIVE')}>Maybe</button>
						<button class="btn sm danger" onclick={() => respond(item, 'DECLINED')}>Decline</button>
					</div>
				{/if}
			</div>
		{/each}
	</div>
{/if}

<style>
	.head {
		margin-bottom: 24px;
	}
	.head h1 {
		font-size: clamp(34px, 6vw, 52px);
	}
	.list {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.invite {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 14px;
		padding: 15px 16px;
		flex-wrap: wrap;
	}
	.info {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.row1 {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.method {
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
		padding: 3px 8px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--accent);
	}
	.method.cancel {
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		color: var(--danger);
	}
	.acts {
		display: flex;
		gap: 6px;
	}
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		padding: 36px;
		text-align: center;
	}
	.small {
		font-size: 12px;
	}
</style>
