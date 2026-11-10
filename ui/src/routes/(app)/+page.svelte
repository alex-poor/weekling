<script>
	import { onMount } from 'svelte';
	import { api } from '#lib/api.js';
	import { session } from '#lib/session.svelte.js';

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

	function textOn(hex) {
		const h = (hex ?? '#3b5bff').replace('#', '').slice(0, 6);
		const r = parseInt(h.slice(0, 2), 16) || 0;
		const g = parseInt(h.slice(2, 4), 16) || 0;
		const b = parseInt(h.slice(4, 6), 16) || 0;
		return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.62 ? '#111' : '#fff';
	}
</script>

<header class="head">
	<h1>{greeting}, {session.user?.display_name?.split(' ')[0] ?? 'there'}<span class="dot">.</span></h1>
	<p class="muted">Here's your weekling at a glance.</p>
</header>

<section class="stats">
	<div class="card stat">
		<span class="muted">Calendars</span>
		<strong class="big">{loaded ? calendars.length : '—'}</strong>
	</div>
	<a class="card stat action" href="/calendars">
		<span class="muted">Manage</span>
		<strong>Calendars →</strong>
	</a>
	<a class="card stat action" href="/connect">
		<span class="muted">Set up</span>
		<strong>Connect a device →</strong>
	</a>
</section>

<h2 class="section-title">Your calendars</h2>
{#if loaded && calendars.length === 0}
	<div class="empty">
		<img src="/logo.png?v=2" alt="" class="mascot-sm" />
		<p>No calendars yet.</p>
		<a class="btn primary" href="/calendars">+ New calendar</a>
	</div>
{:else}
	<div class="grid">
		{#each calendars as cal (cal.id)}
			<a
				class="mini"
				href="/calendars"
				style="background:{cal.color ?? 'var(--blue)'}; color:{textOn(cal.color)}"
			>
				<strong>{cal.display_name}</strong>
				<span class="comps">{cal.components.join(' · ')}</span>
			</a>
		{/each}
	</div>
{/if}

<style>
	.head {
		margin-bottom: 28px;
	}
	.head h1 {
		font-size: clamp(34px, 6vw, 52px);
	}
	.head p {
		margin: 8px 0 0;
	}
	.stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 16px;
		margin-bottom: 36px;
	}
	.stat {
		padding: 20px 22px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.stat .muted {
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 13px;
	}
	.stat strong {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: 19px;
	}
	.stat strong.big {
		font-size: 38px;
	}
	.action {
		text-decoration: none;
		transition:
			transform 0.08s ease,
			box-shadow 0.14s ease;
	}
	.action:hover {
		transform: translateY(-2px);
	}
	.section-title {
		font-family: var(--font-display);
		font-size: 18px;
		margin: 0 0 16px;
		font-weight: 800;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
		gap: 16px;
	}
	.mini {
		padding: 18px 20px;
		border-radius: 18px;
		box-shadow: var(--shadow);
		text-decoration: none;
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-height: 96px;
		justify-content: flex-end;
		transition: transform 0.08s ease;
	}
	.mini:hover {
		transform: translateY(-2px) rotate(-1deg);
	}
	.mini strong {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: 20px;
	}
	.comps {
		font-size: 12.5px;
		opacity: 0.85;
	}
	.empty {
		padding: 44px 30px;
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 14px;
	}
	.mascot-sm {
		width: 118px;
		height: auto;
		transform: rotate(-4deg);
	}
	@media (max-width: 620px) {
		.stats {
			grid-template-columns: 1fr;
		}
	}
</style>
