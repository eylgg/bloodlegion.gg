<script lang="ts">
	import type { Snippet } from 'svelte';
	import Button from './Button.svelte';

	// A button that asks before it acts, in place rather than in a browser dialog: the first press
	// swaps it for the question with a confirm and a cancel button. Escape or Cancel puts it back.
	let {
		question,
		confirmLabel = 'Yes',
		variant = 'secondary',
		confirmVariant = 'danger',
		size = 'default',
		disabled = false,
		onconfirm,
		children
	}: {
		question: string;
		confirmLabel?: string;
		variant?: 'primary' | 'secondary' | 'danger';
		confirmVariant?: 'primary' | 'secondary' | 'danger';
		size?: 'default' | 'small';
		disabled?: boolean;
		onconfirm: () => unknown;
		children?: Snippet;
	} = $props();

	let asking = $state(false);
	let confirmButton = $state<HTMLElement>();

	$effect(() => {
		if (asking) confirmButton?.querySelector('button')?.focus();
	});

	function answer(yes: boolean) {
		asking = false;
		if (yes) onconfirm();
	}
</script>

{#if asking}
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<span
		class="ask"
		role="group"
		aria-label={question}
		onkeydown={(event) => {
			if (event.key === 'Escape') answer(false);
		}}
	>
		<span class="question">{question}</span>
		<span bind:this={confirmButton}>
			<Button variant={confirmVariant} {size} onclick={() => answer(true)}>{confirmLabel}</Button>
		</span>
		<Button variant="secondary" {size} onclick={() => answer(false)}>Cancel</Button>
	</span>
{:else}
	<Button {variant} {size} {disabled} onclick={() => (asking = true)}>
		{@render children?.()}
	</Button>
{/if}

<style>
	.ask {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.question {
		font-size: var(--text-sm);
		color: var(--foreground);
	}
</style>
