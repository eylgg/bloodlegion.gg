<!--
	The full-viewport mark every page shows for now.

	The SVG's square canvas carries its own clear space (the artwork spans 84% of it), so on a
	phone the image runs the full width and still breathes. On larger screens it is capped by
	height and by an absolute size so it sits in the viewport rather than filling it. A faint red
	spill behind it keeps the page from reading as flat black.
-->
<div class="stage">
	<img src="/logo.svg" alt="Blood Legion" width="1024" height="1024" fetchpriority="high" />
</div>

<style>
	.stage {
		min-height: 100vh;
		min-height: 100dvh;
		display: grid;
		place-items: center;
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
