<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { formatDate } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import QuestionForm from '../QuestionForm.svelte';
	import Tally from '../Tally.svelte';
	import type { Question } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let question = $derived<Question>(data.detail.question);
	let editing = $state(false);
	let error = $state('');

	async function remove() {
		if (!confirm('Delete this question and its answers?')) return;
		try {
			await api.del(`/api/questions/${question.id}`);
			await goto(resolve('/questions'));
		} catch (err) {
			error = errorMessage(err, 'Deleting failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>{question.title} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Asked {formatDate(question.created_at)}</p>
		<h1>{question.title}</h1>
	</div>
	{#if data.officer && !editing}
		<div class="actions">
			<Button variant="secondary" size="small" onclick={() => (editing = true)}>Edit</Button>
			<Button variant="danger" size="small" onclick={remove}>Delete</Button>
		</div>
	{/if}
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if editing}
	<QuestionForm
		{question}
		onsaved={(saved) => {
			question = saved;
			editing = false;
		}}
		oncancel={() => (editing = false)}
	/>
{:else if question.body}
	<p class="body">{question.body}</p>
{/if}

<!-- Officers see the names change too, so the page is reloaded after an answer. -->
<Tally {question} onanswered={() => invalidateAll()} />

{#if data.detail.answers}
	<section>
		<h2>Answers</h2>
		{#if data.detail.answers.length === 0}
			<p class="muted">Nobody has answered yet.</p>
		{:else}
			<div class="columns">
				{#each [true, false] as choice (choice)}
					{@const names = data.detail.answers.filter((a) => a.choice === choice)}
					<div class="panel">
						<h3>{choice ? 'Yes' : 'No'} <span class="muted">{names.length}</span></h3>
						<ul>
							{#each names as answer (answer.user_id)}
								<li>{answer.username}</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}

<style>
	.actions {
		display: flex;
		gap: var(--space-2);
	}

	.body {
		max-width: 48rem;
		white-space: pre-wrap;
	}

	.columns {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: var(--space-3);
		max-width: 48rem;
	}

	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	ul {
		margin: 0;
		padding-left: var(--space-4);
	}
</style>
