<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { Question } from '$lib/types';

	let {
		question = null,
		onsaved,
		oncancel
	}: {
		question?: Question | null;
		onsaved: (q: Question) => void;
		oncancel: () => void;
	} = $props();

	// svelte-ignore state_referenced_locally
	let title = $state(question?.title ?? '');
	// svelte-ignore state_referenced_locally
	let body = $state(question?.body ?? '');
	let saving = $state(false);
	let error = $state('');

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = '';
		try {
			onsaved(
				question
					? await api.put<Question>(`/api/questions/${question.id}`, { title, body })
					: await api.post<Question>('/api/questions', { title, body })
			);
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<form class="panel" onsubmit={save}>
	<h3>{question ? 'Edit the question' : 'Ask the guild'}</h3>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	<label class="field">
		Question
		<input
			type="text"
			bind:value={title}
			maxlength="120"
			required
			placeholder="Can you raid on Thursdays?"
		/>
	</label>
	<label class="field">
		Details (optional)
		<textarea bind:value={body} maxlength="4000"></textarea>
	</label>
	<div class="form-actions">
		<Button type="button" variant="secondary" onclick={oncancel}>Cancel</Button>
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : question ? 'Save' : 'Ask'}
		</Button>
	</div>
</form>

<style>
	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}
</style>
