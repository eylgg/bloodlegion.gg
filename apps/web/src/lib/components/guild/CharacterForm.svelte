<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { classIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import SpecsPicker from './SpecsPicker.svelte';
	import type { GuildCharacter, GuildCharacterInput, SpecsInput, WowClass } from '$lib/types';

	/**
	 * Adds or edits a character. `owners` turns on the officers' owner picker: any member, or
	 * nobody (an unclaimed character, a pug who won loot). Without it the character is the
	 * signed-in member's own.
	 */
	let {
		character = null,
		owners = null,
		firstIsMain = false,
		onsaved,
		oncancel
	}: {
		character?: GuildCharacter | null;
		owners?: { id: number; username: string }[] | null;
		/** A member's first character becomes their main whatever the box says; show it ticked. */
		firstIsMain?: boolean;
		onsaved: (saved: GuildCharacter) => void;
		oncancel: () => void;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);

	// svelte-ignore state_referenced_locally
	let firstName = $state(character?.first_name ?? '');
	// svelte-ignore state_referenced_locally
	let lastName = $state(character?.last_name ?? '');
	// svelte-ignore state_referenced_locally
	let classSlug = $state(character?.class ?? '');
	// svelte-ignore state_referenced_locally
	let isMain = $state(character?.is_main ?? firstIsMain);
	// The owner picker's value: a member's id, or '' for nobody.
	// svelte-ignore state_referenced_locally
	let owner = $state(
		character ? String(character.user_id ?? '') : String(page.data.user?.id ?? '')
	);
	// The specs played and the main; a new class starts them over.
	// svelte-ignore state_referenced_locally
	let specs = $state<SpecsInput>({
		specs: (character?.specs ?? []).map((s) => ({ spec: s.spec, talents: [...s.talents] })),
		main: character?.specs.find((s) => s.is_main)?.spec ?? null
	});
	const selectedClass = $derived(classes.find((c) => c.slug === classSlug));

	function pickClass(slug: string) {
		if (slug !== classSlug) specs = { specs: [], main: null };
		classSlug = slug;
	}

	let saving = $state(false);
	let error = $state('');

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!classSlug) {
			error = 'Pick a class.';
			return;
		}
		saving = true;
		error = '';
		const body: GuildCharacterInput = {
			first_name: firstName,
			last_name: lastName,
			class: classSlug,
			is_main: isMain && (!owners || owner !== '')
		};
		if (owners) body.user_id = owner === '' ? null : Number(owner);
		try {
			const saved = character
				? await api.put<GuildCharacter>(`/api/characters/${character.id}`, body)
				: await api.post<GuildCharacter>('/api/characters', body);
			onsaved(await api.put<GuildCharacter>(`/api/characters/${saved.id}/specs`, specs));
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<form class="panel" onsubmit={save}>
	<h3>
		{character ? `Edit ${character.first_name} ${character.last_name}` : 'Add a character'}
	</h3>
	{#if error}<Alert variant="error">{error}</Alert>{/if}

	<div class="row">
		<label class="field">
			First name
			<input type="text" bind:value={firstName} required minlength="2" maxlength="24" />
		</label>
		<label class="field">
			Last name
			<input type="text" bind:value={lastName} required minlength="2" maxlength="24" />
		</label>
	</div>

	<fieldset>
		<legend>Class</legend>
		<div class="classes">
			{#each classes as wowClass (wowClass.slug)}
				<button
					type="button"
					class="class-choice"
					class:selected={wowClass.slug === classSlug}
					style:--class-color={wowClass.color}
					aria-pressed={wowClass.slug === classSlug}
					onclick={() => pickClass(wowClass.slug)}
				>
					<img src={classIcon(wowClass.slug)} alt="" width="24" height="24" />
					<span>{wowClass.name}</span>
				</button>
			{/each}
		</div>
	</fieldset>

	{#if selectedClass}
		<SpecsPicker wowClass={selectedClass} bind:value={specs} />
	{/if}

	{#if owners}
		<label class="field">
			Played by
			<select bind:value={owner}>
				<option value="">No one (not linked to a member)</option>
				{#each owners as member (member.id)}
					<option value={String(member.id)}>{member.username}</option>
				{/each}
			</select>
		</label>
	{/if}

	{#if !owners || owner !== ''}
		<label class="main-toggle">
			<input type="checkbox" bind:checked={isMain} />
			<span>This is {owners ? 'their' : 'my'} main</span>
		</label>
	{/if}

	<div class="form-actions">
		<Button type="button" variant="secondary" onclick={oncancel}>Cancel</Button>
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : character ? 'Save' : 'Add'}
		</Button>
	</div>
</form>

<style>
	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	fieldset {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: 0;
	}

	legend {
		margin-bottom: var(--space-1);
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-weight: 500;
	}

	.classes {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(8rem, 1fr));
		gap: var(--space-2);
	}

	.class-choice {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-2) var(--space-1) var(--space-1);
		color: var(--class-color);
		font: inherit;
		font-size: var(--text-md);
		font-weight: 600;
		text-align: left;
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.class-choice img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.class-choice:hover,
	.class-choice.selected {
		border-color: var(--class-color);
	}

	.class-choice.selected {
		background-color: color-mix(in oklch, var(--class-color) 16%, var(--background));
	}

	.main-toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
		cursor: pointer;
	}
</style>
