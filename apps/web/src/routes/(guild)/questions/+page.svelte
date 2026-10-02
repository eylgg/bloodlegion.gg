<script lang="ts">
	import { resolve } from '$app/paths';
	import { formatDate } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import QuestionForm from './QuestionForm.svelte';
	import Tally from './Tally.svelte';
	import type { Question } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let questions = $state<Question[]>(data.questions);
	let asking = $state(false);

	const replace = (q: Question) => (questions = questions.map((x) => (x.id === q.id ? q : x)));
</script>

<svelte:head>
	<title>Questions | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Questions</h1>
	</div>
	{#if data.officer && !asking}
		<Button variant="primary" onclick={() => (asking = true)}>Ask the guild</Button>
	{/if}
</div>

{#if asking}
	<QuestionForm
		onsaved={(q) => {
			questions = [q, ...questions];
			asking = false;
		}}
		oncancel={() => (asking = false)}
	/>
{/if}

<ul class="questions">
	{#each questions as question (question.id)}
		<li class="panel">
			<div>
				<a href={resolve('/(guild)/questions/[id]', { id: String(question.id) })}
					>{question.title}</a
				>
				<span class="muted small">{formatDate(question.created_at)}</span>
			</div>
			<Tally {question} onanswered={replace} />
		</li>
	{:else}
		<li class="muted">Nothing asked yet.</li>
	{/each}
</ul>

<style>
	.questions {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 48rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.questions li.panel {
		gap: var(--space-3);
		padding: var(--space-4);
	}

	.questions li > div {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	a {
		color: var(--foreground);
		font-size: var(--text-lg);
		font-weight: 600;
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
