<script lang="ts">
	import { formatDateTime } from '$lib/guild';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Notes | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Officers</p>
		<h1>Notes</h1>
	</div>
</div>

<p class="muted">What the raiding roster wrote on their characters, latest first.</p>

<ul class="notes">
	{#each data.notes as note (note.character_id)}
		<li class="panel">
			<div class="head">
				<CharacterLink
					id={note.character_id}
					firstName={note.first_name}
					lastName={note.last_name}
					cls={note.class}
				/>
				<span class="muted small">{note.username} · {formatDateTime(note.updated_at)}</span>
			</div>
			<p>{note.body}</p>
		</li>
	{:else}
		<li class="muted">No notes yet.</li>
	{/each}
</ul>

<style>
	.notes {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 48rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.notes li.panel {
		gap: var(--space-2);
		padding: var(--space-4);
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	p {
		white-space: pre-wrap;
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
