<script lang="ts">
	import type { Ask } from '$lib/ask.svelte';
	import Button from './Button.svelte';

	// The open question of an `Ask`, as a bar in the page with a confirm and a cancel button.
	// Escape cancels.
	let {
		ask,
		confirmLabel = 'Yes',
		confirmVariant = 'primary'
	}: {
		ask: Ask;
		confirmLabel?: string;
		confirmVariant?: 'primary' | 'secondary' | 'danger';
	} = $props();

	let confirmButton = $state<HTMLElement>();

	$effect(() => {
		if (ask.question !== null) confirmButton?.querySelector('button')?.focus();
	});
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && ask.answer(false)} />

{#if ask.question !== null}
	<div class="ask" role="alertdialog" aria-label={ask.question}>
		<span class="question">{ask.question}</span>
		<span class="buttons">
			<span bind:this={confirmButton}>
				<Button variant={confirmVariant} size="small" onclick={() => ask.answer(true)}>
					{confirmLabel}
				</Button>
			</span>
			<Button variant="secondary" size="small" onclick={() => ask.answer(false)}>Cancel</Button>
		</span>
	</div>
{/if}

<style>
	/* Styled like an info `Alert`. */
	.ask {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2) var(--space-3);
		font-size: var(--text-sm);
		color: var(--accent-text);
		background-color: var(--accent-bg);
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--accent-soft);
		border-radius: var(--radius-md);
	}

	.buttons {
		display: inline-flex;
		gap: var(--space-2);
	}
</style>
