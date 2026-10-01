<script lang="ts">
	import type { Snippet } from 'svelte';

	let { children }: { children?: Snippet } = $props();
</script>

<!--
	The full-viewport mark every page shows for now, with room beneath it for the sign-in button.

	The SVG's square canvas carries its own clear space (the artwork spans 84% of it), so on a
	phone the image runs the full width and still breathes. On larger screens it is capped by
	height and by an absolute size so it sits in the viewport rather than filling it. A faint red
	spill behind it keeps the page from reading as flat black.
-->
<div class="stage">
	<img src="/logo.svg" alt="Blood Legion" width="1024" height="1024" fetchpriority="high" />
	{#if children}
		<div class="below">
			{@render children()}
		</div>
	{/if}
</div>

<style>
	.stage {
		min-height: 100vh;
		min-height: 100dvh;
		display: grid;
		grid-auto-rows: min-content;
		align-content: center;
		justify-items: center;
		gap: var(--space-6);
		padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
			env(safe-area-inset-left);
		background: radial-gradient(circle closest-side at 50% 48%, var(--red-glow), transparent);
	}

	img {
		display: block;
		width: min(100vw, 78vh, 40rem);
		height: auto;
		animation: reveal 600ms ease-out both;
	}

	.below {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		width: min(100vw - 2rem, 20rem);
	}

	@keyframes reveal {
		from {
			opacity: 0;
			transform: scale(0.98);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		img {
			animation: none;
		}
	}
</style>
