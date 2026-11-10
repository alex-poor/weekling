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
	<span class="chip"><img src="/logo.png?v=2" alt="Weekling mascot lifting a barbell" /></span>
	<form class="card panel" onsubmit={submit}>
		<h1>weekling<span class="dot">.</span></h1>
		<p class="muted sub">Your calendars, lifting more than they signed up for.</p>

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
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 24px;
	}
	.chip {
		width: 108px;
		height: 108px;
		display: grid;
		place-items: center;
		background: var(--yellow);
		border: 5px solid #fff;
		border-radius: 26px;
		box-shadow: var(--shadow);
		padding: 10px;
		transform: rotate(-5deg);
		margin-bottom: -26px;
		position: relative;
		z-index: 1;
	}
	.chip img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
	.panel {
		width: 100%;
		max-width: 380px;
		padding: 40px 32px 30px;
		text-align: center;
	}
	h1 {
		font-size: 40px;
		line-height: 1;
	}
	.sub {
		margin: 8px 0 22px;
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
