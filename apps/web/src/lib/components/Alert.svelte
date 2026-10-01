<script lang="ts">
	import type { Snippet } from 'svelte';

	type Variant = 'error' | 'info' | 'success';

	let { variant = 'error', children }: { variant?: Variant; children?: Snippet } = $props();
</script>

<p class="alert alert-{variant}" role="alert" aria-live="assertive">
	{@render children?.()}
</p>

<style>
	.alert {
		font-size: var(--text-sm);
		padding: var(--space-2) var(--space-3);
		border: 1px solid transparent;
		border-radius: var(--radius-md);
	}

	.alert-error {
		color: var(--red-text);
		background-color: var(--red-bg);
		border-color: var(--red-soft);
	}

	.alert-info {
		color: var(--accent-text);
		background-color: var(--accent-bg);
		border-color: var(--accent-soft);
	}

	.alert-success {
		color: var(--green-text);
		background-color: var(--green-bg);
		border-color: var(--green-soft);
	}
</style>
