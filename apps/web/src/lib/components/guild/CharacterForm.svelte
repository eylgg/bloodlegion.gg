<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { classIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import SpecsPicker from './SpecsPicker.svelte';
	import { RULESETS } from '$lib/guild';
	import type {
		GuildCharacter,
		GuildCharacterInput,
		GuildSettings,
		Race,
		Ruleset,
		Sex,
		SpecsInput,
		WowClass
	} from '$lib/types';

	/**
	 * Adds or edits a character. `owners` turns on the officers' owner picker: any member, or
	 * nobody (an unclaimed character, a pug who won loot). Without it the character is the
	 * signed-in member's own.
	 */
	let {
		character = null,
		draft = null,
		owners = null,
		firstIsMain = false,
		onsaved,
		oncancel
	}: {
		character?: GuildCharacter | null;
		/** What a new character starts with (a roster member's name, class, and level). */
		draft?: { first_name: string; last_name: string; class: string; level: number } | null;
		owners?: { id: number; username: string }[] | null;
		/** A member's first character becomes their main whatever the box says; show it ticked. */
		firstIsMain?: boolean;
		onsaved: (saved: GuildCharacter) => void;
		oncancel: () => void;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const races = $derived((page.data.races as Race[] | undefined) ?? []);
	const settings = $derived(page.data.settings as GuildSettings | undefined);
	// The guild's faction's races first.
	const FACTIONS = $derived(
		[
			{ faction: 'horde', label: 'Horde' },
			{ faction: 'alliance', label: 'Alliance' }
		].sort(
			(a, b) => Number(b.faction === settings?.faction) - Number(a.faction === settings?.faction)
		)
	);

	// svelte-ignore state_referenced_locally
	let firstName = $state(character?.first_name ?? draft?.first_name ?? '');
	// svelte-ignore state_referenced_locally
	let lastName = $state(character?.last_name ?? draft?.last_name ?? '');
	// svelte-ignore state_referenced_locally
	let classSlug = $state(character?.class ?? draft?.class ?? '');
	// The race's slug, or '' for none recorded.
	// svelte-ignore state_referenced_locally
	let raceSlug = $state(character?.race ?? '');
	// svelte-ignore state_referenced_locally
	let sex = $state<Sex>(character?.sex ?? 'female');
	// svelte-ignore state_referenced_locally
	let ruleset = $state<Ruleset>(character?.ruleset ?? settings?.ruleset ?? 'pvp');
	// svelte-ignore state_referenced_locally
	let level = $state(character?.level ?? draft?.level ?? 1);
	// When it reached 60, as a `datetime-local` value in the reader's own time; '' for now.
	// svelte-ignore state_referenced_locally
	let reached60 = $state(character?.reached_60_at ? localInput(character.reached_60_at) : '');

	function localInput(iso: string): string {
		const date = new Date(iso);
		return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
	}
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
			race: raceSlug || null,
			sex,
			ruleset,
			level,
			reached_60_at: level === 60 && reached60 ? new Date(reached60).toISOString() : null,
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

	<div class="row">
		<label class="field">
			Race
			<select bind:value={raceSlug}>
				<option value="">Not recorded</option>
				{#each FACTIONS as { faction, label } (faction)}
					<optgroup {label}>
						{#each races.filter((r) => r.faction === faction) as race (race.slug)}
							<option value={race.slug}>{race.name}</option>
						{/each}
					</optgroup>
				{/each}
			</select>
		</label>
		<label class="field">
			Sex
			<select bind:value={sex}>
				<option value="female">Female</option>
				<option value="male">Male</option>
			</select>
		</label>
		<label class="field">
			Ruleset
			<select bind:value={ruleset}>
				{#each RULESETS as { ruleset: value, label } (value)}
					<option {value}>{label}</option>
				{/each}
			</select>
		</label>
		<label class="field">
			Level
			<input type="number" bind:value={level} required min="1" max="60" step="1" />
		</label>
	</div>

	{#if level === 60}
		<div class="row">
			<label class="field">
				Reached 60
				<input type="datetime-local" bind:value={reached60} />
				<span class="muted">From their screenshot; empty for now.</span>
			</label>
		</div>
	{/if}

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
