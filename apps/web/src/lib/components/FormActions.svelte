<script lang="ts">
	import type { Snippet } from 'svelte';

	let { align = 'end', children }: { align?: 'start' | 'end'; children?: Snippet } = $props();
</script>

<div class="actions" class:start={align === 'start'}>
	{@render children?.()}
</div>

<style>
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
	}

	.start {
		justify-content: flex-start;
	}
</style>
