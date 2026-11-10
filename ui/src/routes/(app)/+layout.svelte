<script>
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { session } from '#lib/session.svelte.js';
	import { api } from '#lib/api.js';
	import { toast } from '#lib/toast.svelte.js';
	import Icon from '#lib/components/Icon.svelte';

	let { children } = $props();
	let ready = $state(false);

	onMount(async () => {
		try {
			if (!session.user) {
				const me = await api('/me');
				session.user = me.user;
			}
			ready = true;
		} catch {
			goto('/login');
		}
	});

	const nav = [
		{ href: '/', label: 'Dashboard', icon: 'grid' },
		{ href: '/calendars', label: 'Calendars', icon: 'calendar' },
		{ href: '/invitations', label: 'Invitations', icon: 'check' },
		{ href: '/account', label: 'App passwords', icon: 'key' },
		{ href: '/connect', label: 'Connect a device', icon: 'phone' }
	];

	function active(href) {
		const p = page.url.pathname;
		return href === '/' ? p === '/' : p.startsWith(href);
	}

	async function logout() {
		try {
			await api('/logout', { method: 'POST' });
		} catch {
			/* ignore */
		}
		session.user = null;
		goto('/login');
	}
</script>

{#if ready}
	<div class="app">
		<aside class="sidebar">
			<div class="brand">
				<img src="/logo.png" alt="" />
				<span>Weekling</span>
			</div>
			<nav>
				{#each nav as item (item.href)}
					<a href={item.href} class="nav-item" class:active={active(item.href)}>
						<Icon name={item.icon} />
						<span>{item.label}</span>
					</a>
				{/each}
			</nav>
			<div class="foot">
				<div class="who">
					<div class="avatar">{(session.user?.display_name ?? '?').slice(0, 1)}</div>
					<div class="who-text">
						<strong>{session.user?.display_name}</strong>
						<span class="muted">@{session.user?.id}</span>
					</div>
				</div>
				<button class="btn ghost sm" onclick={logout} title="Sign out">
					<Icon name="logout" size={16} /> Sign out
				</button>
			</div>
		</aside>

		<main class="content">
			{@render children()}
		</main>
	</div>
{/if}

{#if toast.visible}
	<div class="toast">{toast.message}</div>
{/if}

<style>
	.app {
		display: grid;
		grid-template-columns: 248px 1fr;
		min-height: 100vh;
	}
	.sidebar {
		border-right: 1px solid var(--line);
		background: var(--surface);
		padding: 20px 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
		position: sticky;
		top: 0;
		height: 100vh;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 4px 8px 14px;
		font-weight: 650;
		font-size: 17px;
	}
	.brand img {
		height: 34px;
		width: auto;
	}
	nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.nav-item {
		display: flex;
		align-items: center;
		gap: 11px;
		padding: 9px 11px;
		border-radius: 10px;
		color: var(--fg);
		font-weight: 550;
		transition: background 0.12s ease;
	}
	.nav-item:hover {
		background: var(--surface-2);
	}
	.nav-item.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
	}
	.foot {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding-top: 12px;
		border-top: 1px solid var(--line);
	}
	.who {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 4px;
	}
	.avatar {
		width: 34px;
		height: 34px;
		border-radius: 50%;
		background: var(--accent);
		color: var(--accent-fg);
		display: grid;
		place-items: center;
		font-weight: 650;
		text-transform: uppercase;
	}
	.who-text {
		display: flex;
		flex-direction: column;
		line-height: 1.2;
		font-size: 13px;
	}
	.content {
		padding: 34px clamp(16px, 4vw, 46px);
		max-width: 960px;
		width: 100%;
	}

	@media (max-width: 760px) {
		.app {
			grid-template-columns: 1fr;
		}
		.sidebar {
			position: static;
			height: auto;
			flex-direction: row;
			flex-wrap: wrap;
			align-items: center;
		}
		nav {
			flex-direction: row;
			flex-wrap: wrap;
			flex: 1;
		}
		.nav-item span {
			display: none;
		}
		.foot {
			margin: 0;
			border: none;
			flex-direction: row;
			padding: 0;
		}
	}
</style>
