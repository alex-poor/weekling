<script>
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.js';
	import { session } from '#lib/session.svelte.js';

	let username = $state('');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);
	let sso = $state(false);

	onMount(async () => {
		try {
			const info = await api('/auth-info');
			sso = !!info.oidc;
		} catch {
			/* leave SSO hidden */
		}
	});

	async function submit(e) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const res = await api('/login', { method: 'POST', body: { username, password } });
			session.user = res.user;
			goto('/');
		} catch (err) {
			error = 'Incorrect username or password.';
		} finally {
			busy = false;
		}
	}
</script>

<div class="wrap">
	<form class="card panel" onsubmit={submit}>
		<img class="logo" src="/logo.png" alt="Weekling" />
		<h1>Welcome to Weekling</h1>
		<p class="muted sub">Sign in to manage your calendars.</p>

		<div class="field">
			<label for="u">Username</label>
			<!-- svelte-ignore a11y_autofocus -->
			<input id="u" class="input" bind:value={username} autocomplete="username" autofocus />
		</div>
		<div class="field">
			<label for="p">Password</label>
			<input
				id="p"
				class="input"
				type="password"
				bind:value={password}
				autocomplete="current-password"
			/>
		</div>

		{#if error}<p class="err">{error}</p>{/if}

		<button class="btn primary full" type="submit" disabled={busy}>
			{busy ? 'Signing in…' : 'Sign in'}
		</button>

		{#if sso}
			<div class="divider"><span>or</span></div>
			<a class="btn full" href="/api/oidc/login">Sign in with SSO</a>
		{/if}
	</form>
</div>

<style>
	.wrap {
		min-height: 100vh;
		display: grid;
		place-items: center;
		padding: 24px;
	}
	.panel {
		width: 100%;
		max-width: 380px;
		padding: 32px;
		text-align: center;
	}
	.logo {
		height: 84px;
		width: auto;
		margin-bottom: 10px;
	}
	h1 {
		font-size: 21px;
	}
	.sub {
		margin: 4px 0 22px;
	}
	.field {
		text-align: left;
	}
	.full {
		width: 100%;
		margin-top: 6px;
	}
	.err {
		color: var(--danger);
		font-size: 13px;
		margin: 0 0 12px;
	}
	.divider {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--muted);
		font-size: 12px;
		margin: 16px 0;
	}
	.divider::before,
	.divider::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--line);
	}
</style>
