<script>
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { session } from '#lib/session.svelte.js';
	import { api } from '#lib/api.js';
	import { toast } from '#lib/toast.svelte.js';

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
		{ href: '/', label: 'Dashboard' },
		{ href: '/calendars', label: 'Calendars' },
		{ href: '/invitations', label: 'Invitations' },
		{ href: '/account', label: 'App passwords' },
		{ href: '/connect', label: 'Connect a device' }
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
		<header class="bar">
			<a class="brand" href="/">
				<span class="logo-chip"><img src="/logo.png?v=2" alt="" /></span>
				<span class="wordmark">weekling</span>
			</a>
			<nav>
				{#each nav as item (item.href)}
					<a href={item.href} class="nav-item" class:active={active(item.href)}>{item.label}</a>
				{/each}
			</nav>
			<div class="user">
				<div class="avatar">{(session.user?.display_name ?? '?').slice(0, 1)}</div>
				<div class="who">
					<strong>{session.user?.display_name}</strong>
					<span class="handle">@{session.user?.id}</span>
				</div>
				<button class="signout" onclick={logout}>Sign out</button>
			</div>
		</header>

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
		max-width: 1440px;
		margin: 0 auto;
		padding: clamp(10px, 2vw, 20px);
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	/* Lemon top bar */
	.bar {
		background: var(--yellow);
		border-radius: var(--radius);
		padding: 12px 18px;
		display: flex;
		align-items: center;
		gap: 14px 22px;
		flex-wrap: wrap;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 11px;
		text-decoration: none;
		color: var(--ink);
	}
	.logo-chip {
		width: 64px;
		height: 64px;
		flex: none;
		display: grid;
		place-items: center;
		transform: rotate(-5deg);
	}
	.logo-chip img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
	.wordmark {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: 24px;
		letter-spacing: -0.02em;
	}

	nav {
		display: flex;
		align-items: center;
		gap: 4px;
		flex: 1;
		flex-wrap: wrap;
	}
	.nav-item {
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 15px;
		color: var(--ink);
		text-decoration: none;
		padding: 8px 15px;
		border-radius: 999px;
		white-space: nowrap;
	}
	.nav-item:hover {
		background: rgba(17, 17, 17, 0.08);
	}
	.nav-item.active {
		background: var(--ink);
		color: #fff;
	}

	.user {
		display: flex;
		align-items: center;
		gap: 10px;
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
	.who {
		display: flex;
		flex-direction: column;
		line-height: 1.15;
		font-size: 13px;
	}
	.who strong {
		font-family: var(--font-display);
		font-weight: 800;
	}
	.handle {
		color: rgba(17, 17, 17, 0.6);
	}
	.signout {
		background: none;
		border: none;
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 13.5px;
		color: var(--ink);
		cursor: pointer;
		text-decoration: underline;
		text-decoration-thickness: 2px;
		text-underline-offset: 2px;
		padding: 4px;
	}

	.content {
		background: var(--paper);
		border-radius: var(--radius);
		box-shadow: var(--shadow);
		padding: clamp(22px, 4vw, 46px);
		min-height: 60vh;
	}

	/* --- Responsive --- */
	@media (max-width: 860px) {
		.bar {
			gap: 12px;
		}
		.brand {
			order: 1;
		}
		.user {
			order: 2;
			margin-left: auto;
		}
		nav {
			order: 3;
			flex-basis: 100%;
			gap: 2px;
			overflow-x: auto;
			flex-wrap: nowrap;
			-webkit-overflow-scrolling: touch;
			scrollbar-width: none;
			padding-bottom: 2px;
		}
		nav::-webkit-scrollbar {
			display: none;
		}
		.nav-item {
			padding: 7px 13px;
			font-size: 14px;
		}
	}
	@media (max-width: 520px) {
		.who {
			display: none;
		}
		.wordmark {
			font-size: 21px;
		}
	}
</style>
