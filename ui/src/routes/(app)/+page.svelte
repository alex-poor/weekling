<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '#lib/components/Icon.svelte';

	let calendars = $state([]);
	let loaded = $state(false);

	onMount(async () => {
		try {
			calendars = await api('/calendars');
		} catch {
			/* handled by api */
		}
		loaded = true;
	});

	const hour = new Date().getHours();
	const greeting = hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
</script>

<header class="head">
	<h1>{greeting}, {session.user?.display_name?.split(' ')[0] ?? 'there'}.</h1>
	<p class="muted">Here's your Weekling at a glance.</p>
</header>

<section class="stats">
	<div class="card stat">
		<span class="muted">Calendars</span>
		<strong>{loaded ? calendars.length : '—'}</strong>
	</div>
	<a class="card stat action" href="/calendars">
		<span class="muted">Manage</span>
		<strong class="row"><Icon name="calendar" /> Calendars</strong>
	</a>
	<a class="card stat action" href="/connect">
		<span class="muted">Set up</span>
		<strong class="row"><Icon name="phone" /> Connect a device</strong>
	</a>
</section>

<section>
	<h2 class="section-title">Your calendars</h2>
	{#if loaded && calendars.length === 0}
		<div class="card empty">
			<p>No calendars yet.</p>
			<a class="btn primary" href="/calendars"><Icon name="plus" size={16} /> Create one</a>
		</div>
	{:else}
		<div class="grid">
			{#each calendars as cal (cal.id)}
				<a class="card cal" href="/calendars">
					<span class="dot" style="background:{cal.color ?? 'var(--accent)'}"></span>
					<div>
						<strong>{cal.display_name}</strong>
						<span class="muted small">{cal.components.join(' · ')}</span>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</section>

<style>
	.head {
		margin-bottom: 24px;
	}
	.head h1 {
		font-size: 24px;
	}
	.stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 14px;
		margin-bottom: 30px;
	}
	.stat {
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.stat strong {
		font-size: 22px;
	}
	.stat .row strong,
	.stat.action strong {
		font-size: 16px;
	}
	.action {
		transition: border-color 0.12s ease;
	}
	.action:hover {
		border-color: var(--accent);
	}
	.section-title {
		font-size: 15px;
		color: var(--muted);
		margin-bottom: 12px;
		font-weight: 600;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 14px;
	}
	.cal {
		padding: 16px;
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.cal > div {
		display: flex;
		flex-direction: column;
	}
	.dot {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		flex: none;
	}
	.small {
		font-size: 12px;
	}
	.empty {
		padding: 30px;
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 14px;
	}
	@media (max-width: 620px) {
		.stats {
			grid-template-columns: 1fr;
		}
	}
</style>
