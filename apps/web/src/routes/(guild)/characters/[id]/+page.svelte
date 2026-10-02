<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { formatDateTime } from '$lib/guild';
	import { classIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import type { GuildCharacter, Note } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Edited in place after a save, and reset when the page moves to another character (the
	// component is reused): a writable derived does both.
	let character = $derived<GuildCharacter>(data.detail.character);
	let note = $derived<Note | null>(data.detail.note);
	let editing = $state(false);
	let writing = $state(false);
	let draft = $state('');
	let saving = $state(false);
	let error = $state('');

	const wowClass = $derived(data.classes.find((c) => c.slug === character.class));
	// Officers pick the owner; a player editing their own character does not.
	const isPlayer = $derived(character.user_id === data.user.id);

	async function remove() {
		if (!confirm(`Remove ${character.first_name} ${character.last_name}?`)) return;
		error = '';
		try {
			await api.del(`/api/characters/${character.id}`);
			await goto(resolve('/characters'));
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}

	async function saveNote(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = '';
		try {
			note = await api.put<Note | null>(`/api/characters/${character.id}/note`, { body: draft });
			writing = false;
		} catch (err) {
			error = errorMessage(err, 'Saving the note failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>{character.first_name} {character.last_name} | Blood Legion</title>
</svelte:head>

<div class="hero" style:--class-color={wowClass?.color ?? 'var(--foreground)'}>
	<img src={classIcon(character.class)} alt="" width="72" height="72" />
	<div>
		<p class="kicker">
			{wowClass?.name ?? character.class}{character.is_main
				? ' · Main'
				: character.user_id
					? ' · Alt'
					: ''}
		</p>
		<h1>{character.first_name} {character.last_name}</h1>
		<p class="muted">
			<CharacterSpecs
				cls={character.class}
				primary={character.primary_spec}
				secondary={character.secondary_spec}
			/>
			{character.username ? `Played by ${character.username} · ` : ''}{data.detail.raids_attended}
			{data.detail.raids_attended === 1 ? 'raid' : 'raids'} · {data.detail.loot.length}
			{data.detail.loot.length === 1 ? 'item' : 'items'}
		</p>
	</div>
	{#if data.detail.can_edit && !editing}
		<div class="actions">
			<Button variant="secondary" size="small" onclick={() => (editing = true)}>Edit</Button>
			<Button variant="danger" size="small" onclick={remove}>Remove</Button>
		</div>
	{/if}
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if editing}
	<CharacterForm
		{character}
		owners={data.officer && !isPlayer ? data.owners : null}
		onsaved={(saved) => {
			character = saved;
			editing = false;
		}}
		oncancel={() => (editing = false)}
	/>
{/if}

<section>
	<h2>Loot</h2>
	<LootTable loot={data.detail.loot} winner={false} empty="Nothing won yet." />
</section>

{#if data.detail.can_write_note || note}
	<section>
		<h2>Notes</h2>
		<p class="muted">
			{data.detail.can_write_note
				? 'What you are saving for, when you can raid: only you and the officers see this.'
				: 'Only the player and the officers see this.'}
		</p>
		{#if writing}
			<form class="panel" onsubmit={saveNote}>
				<textarea bind:value={draft} maxlength="10000" aria-label="Notes"></textarea>
				<div class="form-actions">
					<Button type="button" variant="secondary" onclick={() => (writing = false)}>Cancel</Button
					>
					<Button type="submit" variant="primary" disabled={saving}>
						{saving ? 'Saving...' : 'Save'}
					</Button>
				</div>
			</form>
		{:else}
			{#if note}
				<div class="panel note">
					<p>{note.body}</p>
					<span class="muted small">Updated {formatDateTime(note.updated_at)}</span>
				</div>
			{/if}
			{#if data.detail.can_write_note}
				<div>
					<Button
						variant="secondary"
						onclick={() => {
							draft = note?.body ?? '';
							writing = true;
						}}
					>
						{note ? 'Edit notes' : 'Write notes'}
					</Button>
				</div>
			{/if}
		{/if}
	</section>
{/if}

<style>
	.hero {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-4);
	}

	.hero > div:not(.actions) {
		flex: 1;
	}

	.hero img {
		display: block;
		border-radius: var(--radius-lg);
		box-shadow: 0 0 0 1px color-mix(in oklch, var(--class-color) 60%, black);
	}

	.hero h1 {
		color: var(--class-color);
	}

	.actions {
		display: flex;
		gap: var(--space-2);
	}

	.note p {
		white-space: pre-wrap;
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
