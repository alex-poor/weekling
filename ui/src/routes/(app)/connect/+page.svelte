<script>
	import { onMount } from 'svelte';
	import { session } from '#lib/session.svelte.js';
	import { showToast } from '#lib/toast.svelte.js';
	import Icon from '#lib/components/Icon.svelte';

	let origin = $state('');
	onMount(() => {
		origin = location.origin;
	});

	function copy(text) {
		navigator.clipboard?.writeText(text);
		showToast('Copied');
	}

	const clients = [
		{
			name: 'Apple Calendar (macOS / iOS)',
			steps: [
				'Settings → Calendar → Accounts → Add Account → Other → Add CalDAV Account',
				'Account Type: Advanced',
				'Server address: the URL below · User Name: your username · Password: an app password'
			]
		},
		{
			name: 'Thunderbird',
			steps: [
				'New Calendar → On the Network',
				'Paste the URL below and enter your username',
				'When prompted for a password, use an app password'
			]
		},
		{
			name: 'DAVx⁵ (Android)',
			steps: [
				'Add account → Login with URL and user name',
				'Base URL: the URL below · User name & password: your username + an app password'
			]
		}
	];
</script>

<header class="head">
	<h1>Connect a device</h1>
	<p class="muted">Point any CalDAV app at Weekling. Discovery is automatic from the base URL.</p>
</header>

<div class="card url-card">
	<div>
		<span class="muted small">Server URL</span>
		<code>{origin}/</code>
	</div>
	<button class="btn" onclick={() => copy(origin + '/')}><Icon name="copy" size={15} /> Copy</button>
</div>

<div class="creds">
	<div class="card mini">
		<span class="muted small">Username</span>
		<strong>{session.user?.id}</strong>
	</div>
	<div class="card mini">
		<span class="muted small">Password</span>
		<strong
			>An <a href="/account">app password</a></strong
		>
	</div>
</div>

<div class="clients">
	{#each clients as c (c.name)}
		<div class="card client">
			<h2>{c.name}</h2>
			<ol>
				{#each c.steps as s, i (i)}
					<li>{s}</li>
				{/each}
			</ol>
		</div>
	{/each}
</div>

<p class="muted foot">
	Tip: most clients find the right calendars automatically via
	<code>/.well-known/caldav</code>. If a client asks for a full calendar path, open the
	<a href="/calendars">Calendars</a> page and copy its URL.
</p>

<style>
	.head {
		margin-bottom: 22px;
	}
	.head h1 {
		font-size: 24px;
	}
	.url-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 14px;
		padding: 16px 18px;
		margin-bottom: 14px;
	}
	.url-card code {
		display: block;
		font-size: 18px;
		margin-top: 2px;
	}
	.small {
		font-size: 12px;
	}
	.creds {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 14px;
		margin-bottom: 26px;
	}
	.mini {
		padding: 14px 16px;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.mini strong {
		font-size: 16px;
	}
	.clients {
		display: grid;
		gap: 14px;
	}
	.client {
		padding: 18px 20px;
	}
	.client h2 {
		font-size: 16px;
		margin-bottom: 10px;
	}
	.client ol {
		margin: 0;
		padding-left: 20px;
		color: var(--muted);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.client li {
		padding-left: 4px;
	}
	.foot {
		margin-top: 22px;
		font-size: 13px;
	}
	@media (max-width: 560px) {
		.creds {
			grid-template-columns: 1fr;
		}
	}
</style>
