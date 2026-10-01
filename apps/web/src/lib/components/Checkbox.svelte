<script lang="ts">
	import type { HTMLInputAttributes } from 'svelte/elements';
	import type { Snippet } from 'svelte';

	type Props = Omit<HTMLInputAttributes, 'type' | 'checked'> & {
		checked?: boolean;
		children?: Snippet;
	};

	let { checked = $bindable(false), children, ...rest }: Props = $props();
</script>

<label class="checkbox">
	<input type="checkbox" bind:checked {...rest} />
	<span>{@render children?.()}</span>
</label>

<style>
	.checkbox {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	input {
		margin-top: 0.2em;
		accent-color: var(--accent-solid);
	}
</style>
