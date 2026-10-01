<script lang="ts">
	import { resolve } from '$app/paths';

	let { children } = $props();
</script>

<!-- Pages that stand alone: sign-in and the OAuth2 consent screen. The wordmark above them links
     home; the page itself is a single centred card. -->
<div class="standalone">
	<a href={resolve('/')} class="brand">
		<img src="/wordmark.svg" alt="Blood Legion" width="659" height="201" />
	</a>
	<main>
		{@render children?.()}
	</main>
</div>

<style>
	.standalone {
		min-height: 100vh;
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		justify-content: center;
		align-items: center;
		gap: var(--space-8);
		padding: var(--space-8) var(--space-4);
		background: radial-gradient(circle closest-side at 50% 30%, var(--red-glow), transparent);
	}

	.brand img {
		display: block;
		width: min(100%, 16rem);
		height: auto;
	}

	main {
		width: 100%;
		max-width: 24rem;
	}
</style>
