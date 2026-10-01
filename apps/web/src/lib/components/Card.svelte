<script lang="ts">
	import type { Snippet } from 'svelte';

	let { children }: { children?: Snippet } = $props();
</script>

<section class="card">
	{@render children?.()}
</section>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		width: 100%;
		padding: var(--space-8);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-xl);
	}
</style>
