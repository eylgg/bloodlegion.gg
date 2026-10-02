<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import type { Question } from '$lib/types';

	/** A question's yes/no tally, with the member's own answer as two buttons. */
	let { question, onanswered }: { question: Question; onanswered: (q: Question) => void } =
		$props();

	let error = $state('');

	async function answer(choice: boolean) {
		error = '';
		try {
			onanswered(await api.put<Question>(`/api/questions/${question.id}/answer`, { choice }));
		} catch (err) {
			error = errorMessage(err, 'Answering failed. Please try again.');
		}
	}

	const total = $derived(question.yes + question.no);
</script>

<div class="tally">
	<div class="buttons" role="group" aria-label="Your answer">
		<Button
			size="small"
			variant={question.answer === true ? 'primary' : 'secondary'}
			aria-pressed={question.answer === true}
			onclick={() => answer(true)}>Yes</Button
		>
		<Button
			size="small"
			variant={question.answer === false ? 'primary' : 'secondary'}
			aria-pressed={question.answer === false}
			onclick={() => answer(false)}>No</Button
		>
	</div>
	<span class="bar" style:--yes={total ? question.yes / total : 0} aria-hidden="true"></span>
	<span class="counts">{question.yes} yes · {question.no} no</span>
	{#if error}<span class="error">{error}</span>{/if}
</div>

<style>
	.tally {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}

	.buttons {
		display: flex;
		gap: var(--space-1);
	}

	.bar {
		flex: 0 1 10rem;
		height: 0.5rem;
		background: linear-gradient(
			90deg,
			var(--green-text) calc(var(--yes) * 100%),
			var(--grey-surface) 0
		);
		border-radius: 999px;
	}

	.counts {
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
	}

	.error {
		color: var(--red-text);
		font-size: var(--text-sm);
	}
</style>
