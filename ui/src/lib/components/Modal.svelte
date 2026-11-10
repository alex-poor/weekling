<script>
	let { title, onclose, children } = $props();

	function onkey(e) {
		if (e.key === 'Escape') onclose();
	}
</script>

<svelte:window onkeydown={onkey} />

<div class="backdrop" onclick={onclose} role="presentation">
	<div class="modal card" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
		<div class="spread mhead">
			<h2>{title}</h2>
			<button class="btn ghost sm" onclick={onclose} aria-label="Close">✕</button>
		</div>
		{@render children()}
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(10, 12, 18, 0.45);
		backdrop-filter: blur(2px);
		display: grid;
		place-items: center;
		padding: 20px;
		z-index: 40;
	}
	.modal {
		width: 100%;
		max-width: 420px;
		padding: 22px;
	}
	.mhead {
		margin-bottom: 16px;
	}
	.mhead h2 {
		font-size: 18px;
	}
</style>
