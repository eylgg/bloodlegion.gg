<script lang="ts">
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import type { Character, CharacterInput, Role, RosterEntry, WowClass } from '$lib/types';
	import { api, errorMessage } from '$lib/api';
	import { classIcon, specIcon, ROLES, ROLE_LABEL } from '$lib/wow/icons';
	import RoleIcon from '$lib/components/RoleIcon.svelte';
	import LaunchCountdown from '$lib/components/LaunchCountdown.svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Alert from '$lib/components/Alert.svelte';

	let { data }: PageProps = $props();

	// Both lists are edited in place after each save, so they start from the load and live here.
	// svelte-ignore state_referenced_locally
	let characters = $state<Character[]>(data.characters);
	// svelte-ignore state_referenced_locally
	let roster = $state<RosterEntry[]>(data.roster);

	const classBySlug = $derived(new Map(data.classes.map((c) => [c.slug, c])));

	function specOf(wowClass: WowClass | undefined, slug: string) {
		return wowClass?.specs.find((s) => s.slug === slug);
	}

	// Players are people, counted by username; a player's characters (main and alts) sit under
	// them. Alphabetical, case-insensitive.
	const byUsername = (a: string, b: string) =>
		a.localeCompare(b, undefined, { sensitivity: 'base' });

	const playerCount = $derived(new Set(roster.map((e) => e.username)).size);

	type PlayerRow = {
		username: string;
		// The player's characters that fill this role, each with only the specs that do. Main
		// first.
		characters: { entry: RosterEntry; specs: string[] }[];
		// Whether their main fills it, so a column can tell "can bring it on day one" from "has an
		// alt for it".
		onMain: boolean;
	};

	// The composition board: under each role, every player who can fill it, with the characters
	// they would fill it on.
	const board = $derived.by(() => {
		const columns = Object.fromEntries(ROLES.map((role) => [role, [] as PlayerRow[]])) as Record<
			Role,
			PlayerRow[]
		>;
		for (const role of ROLES) {
			const rows: Record<string, PlayerRow> = {};
			for (const entry of roster) {
				const wowClass = classBySlug.get(entry.class);
				const specs = entry.specs.filter((s) => specOf(wowClass, s)?.roles.includes(role));
				if (specs.length === 0) continue;
				const row = (rows[entry.username] ??= {
					username: entry.username,
					characters: [],
					onMain: false
				});
				row.characters.push({ entry, specs });
				row.onMain ||= entry.is_main;
			}
			columns[role] = Object.values(rows)
				.map((row) => ({
					...row,
					characters: row.characters.sort(
						(a, b) => Number(b.entry.is_main) - Number(a.entry.is_main)
					)
				}))
				.sort((a, b) => byUsername(a.username, b.username));
		}
		return columns;
	});

	const undecided = $derived(
		roster
			.filter((entry) => entry.specs.length === 0)
			.sort((a, b) => byUsername(a.username, b.username))
	);

	// Mains per class, to show the spread and its gaps.
	const classSpread = $derived(
		data.classes.map((wowClass) => ({
			wowClass,
			mains: roster.filter((e) => e.is_main && e.class === wowClass.slug).length,
			alts: roster.filter((e) => !e.is_main && e.class === wowClass.slug).length
		}))
	);

	// The editor: `editing` is the sign-up being changed, or null when reserving a new one.
	let open = $state(false);
	let editing = $state<Character | null>(null);
	let name = $state('');
	let classSlug = $state('');
	let specs = $state<string[]>([]);
	let isMain = $state(false);
	let saving = $state(false);
	let error = $state('');

	const selectedClass = $derived(classBySlug.get(classSlug));

	function startNew() {
		editing = null;
		name = '';
		classSlug = '';
		specs = [];
		// A first sign-up is the main whatever the box says; showing it ticked says so.
		isMain = characters.length === 0;
		error = '';
		open = true;
	}

	function startEdit(character: Character) {
		editing = character;
		name = character.name;
		classSlug = character.class;
		specs = [...character.specs];
		isMain = character.is_main;
		error = '';
		open = true;
	}

	function pickClass(slug: string) {
		if (slug !== classSlug) specs = [];
		classSlug = slug;
	}

	function toggleSpec(slug: string) {
		specs = specs.includes(slug) ? specs.filter((s) => s !== slug) : [...specs, slug];
	}

	function sortCharacters(list: Character[]): Character[] {
		return [...list].sort(
			(a, b) => Number(b.is_main) - Number(a.is_main) || a.created_at.localeCompare(b.created_at)
		);
	}

	// After any change, the guild list is fetched again rather than patched by hand: it is small,
	// and it may also show other members' new sign-ups.
	async function refreshRoster() {
		try {
			roster = await api.get<RosterEntry[]>('/api/launch/roster');
		} catch {
			// The page still shows the member's own sign-ups; the guild list catches up on reload.
		}
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!classSlug) {
			error = 'Pick a class.';
			return;
		}
		saving = true;
		error = '';
		const body: CharacterInput = { name, class: classSlug, specs, is_main: isMain };
		try {
			const saved = editing
				? await api.put<Character>(`/api/launch/characters/${editing.id}`, body)
				: await api.post<Character>('/api/launch/characters', body);
			// Saving a main demotes the others on the server; mirror that here.
			const others = characters
				.filter((c) => c.id !== saved.id)
				.map((c) => (saved.is_main ? { ...c, is_main: false } : c));
			characters = sortCharacters([...others, saved]);
			open = false;
			await refreshRoster();
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}

	async function makeMain(character: Character) {
		try {
			const saved = await api.put<Character>(`/api/launch/characters/${character.id}`, {
				name: character.name,
				class: character.class,
				specs: character.specs,
				is_main: true
			});
			characters = sortCharacters(
				characters.map((c) => (c.id === saved.id ? saved : { ...c, is_main: false }))
			);
			await refreshRoster();
		} catch (err) {
			error = errorMessage(err, 'That did not work. Please try again.');
		}
	}

	async function remove(character: Character) {
		if (!confirm(`Remove ${character.name} from your sign-ups?`)) return;
		try {
			await api.del(`/api/launch/characters/${character.id}`);
			characters = characters.filter((c) => c.id !== character.id);
			if (editing?.id === character.id) open = false;
			await refreshRoster();
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>Launch sign-ups | Blood Legion</title>
</svelte:head>

<div class="backdrop">
	<div class="page">
		<a href={resolve('/')} class="brand">
			<img src="/wordmark.svg" alt="Blood Legion" width="659" height="201" />
		</a>

		<section class="hero">
			<p class="kicker">World of Warcraft: Forever</p>
			<h1>Launch sign-ups</h1>
			<p class="lede">
				Reserve the characters you'll make on day one, and the specs you hope to play. Names are
				yours to claim when the realms open.
			</p>
			<LaunchCountdown />
		</section>

		{#if error && !open}
			<Alert variant="error">{error}</Alert>
		{/if}

		<section class="block">
			<h2>Your sign-ups</h2>

			{#if characters.length === 0 && !open}
				<p class="muted">Nothing reserved yet. Start with your main.</p>
			{/if}

			<ul class="characters">
				{#each characters as character (character.id)}
					{@const wowClass = classBySlug.get(character.class)}
					<li class="character" style:--class-color={wowClass?.color ?? 'var(--grey-solid)'}>
						<img
							class="class-icon"
							src={classIcon(character.class)}
							alt=""
							width="56"
							height="56"
						/>
						<div class="identity">
							<div class="name-line">
								<span class="name">{character.name}</span>
								{#if character.is_main}<span class="main-badge">Main</span>{/if}
							</div>
							<span class="class">{wowClass?.name ?? character.class}</span>
							{#if character.specs.length > 0}
								<ul class="specs">
									{#each character.specs as slug (slug)}
										{@const spec = specOf(wowClass, slug)}
										<li>
											<img src={specIcon(character.class, slug)} alt="" width="20" height="20" />
											{spec?.name ?? slug}
										</li>
									{/each}
								</ul>
							{/if}
						</div>
						<div class="actions">
							{#if !character.is_main}
								<Button size="small" variant="secondary" onclick={() => makeMain(character)}>
									Make main
								</Button>
							{/if}
							<Button size="small" variant="secondary" onclick={() => startEdit(character)}>
								Edit
							</Button>
							<Button size="small" variant="danger" onclick={() => remove(character)}>
								Remove
							</Button>
						</div>
					</li>
				{/each}
			</ul>

			{#if open}
				<form class="editor" onsubmit={save}>
					<h3>{editing ? `Edit ${editing.name}` : 'Reserve a character'}</h3>

					{#if error}
						<Alert variant="error">{error}</Alert>
					{/if}

					<Input
						label="Character name"
						name="name"
						required
						minlength={2}
						maxlength={12}
						autocomplete="off"
						spellcheck={false}
						bind:value={name}
					/>

					<fieldset>
						<legend>Class</legend>
						<div class="classes">
							{#each data.classes as wowClass (wowClass.slug)}
								<button
									type="button"
									class="class-choice"
									class:selected={wowClass.slug === classSlug}
									style:--class-color={wowClass.color}
									aria-pressed={wowClass.slug === classSlug}
									onclick={() => pickClass(wowClass.slug)}
								>
									<img src={classIcon(wowClass.slug)} alt="" width="28" height="28" />
									<span>{wowClass.name}</span>
								</button>
							{/each}
						</div>
					</fieldset>

					{#if selectedClass}
						<fieldset>
							<legend>Specs you hope to play</legend>
							<div class="spec-choices" style:--class-color={selectedClass.color}>
								{#each selectedClass.specs as spec (spec.slug)}
									<label class="spec-choice" class:selected={specs.includes(spec.slug)}>
										<input
											type="checkbox"
											checked={specs.includes(spec.slug)}
											onchange={() => toggleSpec(spec.slug)}
										/>
										<img
											src={specIcon(selectedClass.slug, spec.slug)}
											alt=""
											width="32"
											height="32"
										/>
										<span class="spec-name">{spec.name}</span>
										<span class="spec-roles">
											{#each spec.roles as role (role)}
												<RoleIcon {role} size={16} />
											{/each}
										</span>
									</label>
								{/each}
							</div>
						</fieldset>
					{/if}

					<label class="main-toggle">
						<input type="checkbox" bind:checked={isMain} />
						<span>This is my main</span>
					</label>

					<div class="form-actions">
						<Button type="button" variant="secondary" onclick={() => (open = false)}>Cancel</Button>
						<Button type="submit" variant="primary" disabled={saving}>
							{saving ? 'Saving...' : editing ? 'Save' : 'Reserve'}
						</Button>
					</div>
				</form>
			{:else}
				<div>
					<Button variant="primary" onclick={startNew}>
						{characters.length === 0 ? 'Reserve your main' : 'Reserve another character'}
					</Button>
				</div>
			{/if}
		</section>

		<section class="block wide">
			<div class="board-head">
				<h2>The Legion so far</h2>
				<p class="muted">
					{playerCount}
					{playerCount === 1 ? 'player' : 'players'} · {roster.length}
					{roster.length === 1 ? 'character' : 'characters'}
				</p>
			</div>

			{#if roster.length === 0}
				<p class="muted">No one has signed up yet. Be the first.</p>
			{:else}
				<ul class="spread" aria-label="Mains by class">
					{#each classSpread as { wowClass, mains, alts } (wowClass.slug)}
						<li
							class:empty={mains === 0}
							style:--class-color={wowClass.color}
							title="{wowClass.name}: {mains} {mains === 1 ? 'main' : 'mains'}, {alts} {alts === 1
								? 'alt'
								: 'alts'}"
						>
							<img src={classIcon(wowClass.slug)} alt={wowClass.name} width="28" height="28" />
							<span class="spread-count">{mains}</span>
						</li>
					{/each}
				</ul>

				<div class="board">
					{#each ROLES as role (role)}
						{@const players = board[role]}
						{@const onMain = players.filter((p) => p.onMain).length}
						<section class="column">
							<header class="column-head">
								<RoleIcon {role} size={26} decorative />
								<h3>{ROLE_LABEL[role]}</h3>
								<span class="column-count" title="Players who can fill this role"
									>{players.length}</span
								>
							</header>
							{#if players.length === 0}
								<p class="column-empty">Nobody yet</p>
							{:else}
								<p class="column-note">
									{onMain} on their main{players.length > onMain
										? `, ${players.length - onMain} on an alt`
										: ''}
								</p>
								<ul class="players">
									{#each players as player (player.username)}
										<li class="player">
											<span class="player-name">{player.username}</span>
											<ul class="player-characters">
												{#each player.characters as { entry, specs } (entry.name)}
													{@const wowClass = classBySlug.get(entry.class)}
													<li
														class="entry"
														class:alt={!entry.is_main}
														style:--class-color={wowClass?.color ?? 'var(--grey-solid)'}
													>
														<span class="entry-specs">
															{#each specs as slug (slug)}
																{@const spec = specOf(wowClass, slug)}
																<img
																	src={specIcon(entry.class, slug)}
																	alt={spec?.name ?? slug}
																	title="{spec?.name ?? slug} {wowClass?.name ?? ''}"
																	width="20"
																	height="20"
																/>
															{/each}
														</span>
														<span class="entry-name">{entry.name}</span>
														{#if !entry.is_main}<span class="alt-tag">alt</span>{/if}
													</li>
												{/each}
											</ul>
										</li>
									{/each}
								</ul>
							{/if}
						</section>
					{/each}
				</div>

				{#if undecided.length > 0}
					<p class="undecided">
						<span class="muted">Specs not picked yet:</span>
						{#each undecided as entry, i (entry.username + entry.name)}
							{@const wowClass = classBySlug.get(entry.class)}
							<span style:color={wowClass?.color}>{entry.name}</span>
							<span class="muted">({entry.username})</span>{i < undecided.length - 1 ? ', ' : ''}
						{/each}
					</p>
				{/if}
			{/if}
		</section>
	</div>
</div>

<style>
	.page {
		display: flex;
		flex-direction: column;
		gap: var(--space-8);
		width: 100%;
		max-width: 72rem;
		margin: 0 auto;
		padding: var(--space-8) var(--space-4) calc(var(--space-8) * 2);
	}

	/* Scoped to this page: a body-level background would outlive it after navigation. */
	.backdrop {
		min-height: 100vh;
		min-height: 100dvh;
		background: radial-gradient(ellipse 80% 40rem at 50% -10rem, var(--red-glow), transparent);
	}

	.brand img {
		display: block;
		width: min(100%, 11rem);
		height: auto;
	}

	/* --- hero --- */

	.hero {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.kicker {
		color: var(--red-text);
		font-size: var(--text-sm);
		font-weight: 600;
		letter-spacing: 0.12em;
		text-transform: uppercase;
	}

	h1 {
		font-size: clamp(2rem, 6vw, 2.75rem);
		line-height: 1.05;
		letter-spacing: -0.01em;
	}

	.lede {
		max-width: 34rem;
		color: var(--grey-text-active);
	}

	/* --- sections --- */

	.block {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		width: 100%;
		max-width: 44rem;
	}

	.block.wide {
		max-width: none;
	}

	.hero {
		max-width: 44rem;
	}

	h2 {
		font-size: var(--text-xl);
	}

	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	.muted {
		color: var(--grey-text);
		font-size: var(--text-md);
	}

	/* --- your sign-ups --- */

	.characters,
	.specs,
	.spread,
	.players,
	.player-characters {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.characters {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.character {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: start;
		gap: var(--space-4);
		padding: var(--space-4);
		background:
			linear-gradient(
				90deg,
				color-mix(in oklch, var(--class-color) 10%, transparent),
				transparent 60%
			),
			var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--class-color);
		border-radius: var(--radius-lg);
	}

	.class-icon {
		display: block;
		border-radius: var(--radius-md);
		box-shadow: 0 0 0 1px color-mix(in oklch, var(--class-color) 60%, black);
	}

	.identity {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}

	.name-line {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.name {
		color: var(--class-color);
		font-size: var(--text-xl);
		font-weight: 700;
	}

	.class {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.main-badge {
		padding: 0 var(--space-2);
		font-size: var(--text-xs);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--red-text);
		background-color: var(--red-bg);
		border: 1px solid var(--red-soft);
		border-radius: var(--radius-md);
	}

	.specs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-top: var(--space-1);
	}

	.specs li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 2px var(--space-2) 2px 2px;
		font-size: var(--text-sm);
		background-color: var(--background);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-md);
	}

	.specs img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: var(--space-2);
	}

	@media (max-width: 34rem) {
		.character {
			grid-template-columns: auto 1fr;
		}

		.actions {
			grid-column: 1 / -1;
			justify-content: flex-start;
		}
	}

	/* --- editor --- */

	.editor {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-6);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-xl);
	}

	fieldset {
		margin: 0;
		padding: 0;
		border: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	legend {
		margin-bottom: var(--space-1);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--grey-text);
	}

	.classes {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(8.5rem, 1fr));
		gap: var(--space-2);
	}

	.class-choice {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-2) var(--space-1) var(--space-1);
		font: inherit;
		font-size: var(--text-md);
		font-weight: 600;
		color: var(--class-color);
		text-align: left;
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition:
			border-color 0.15s ease,
			background-color 0.15s ease;
	}

	.class-choice img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.class-choice:hover {
		border-color: var(--class-color);
	}

	.class-choice.selected {
		border-color: var(--class-color);
		background-color: color-mix(in oklch, var(--class-color) 16%, var(--background));
	}

	.spec-choices {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
		gap: var(--space-2);
	}

	.spec-choice {
		display: grid;
		grid-template-columns: auto auto 1fr;
		grid-template-rows: auto auto;
		column-gap: var(--space-2);
		align-items: center;
		padding: var(--space-2);
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.spec-choice.selected {
		border-color: var(--class-color);
		background-color: color-mix(in oklch, var(--class-color) 10%, var(--background));
	}

	.spec-choice input {
		grid-row: 1 / span 2;
	}

	.spec-choice > img {
		grid-row: 1 / span 2;
		display: block;
		border-radius: var(--radius-sm);
	}

	.spec-name {
		font-weight: 600;
	}

	.spec-choice input,
	.main-toggle input {
		accent-color: var(--accent-solid);
	}

	.main-toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
		cursor: pointer;
	}

	.form-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
	}

	/* --- the legion --- */

	/* --- the board --- */

	.board-head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2) var(--space-4);
	}

	.spec-roles {
		display: flex;
		gap: var(--space-1);
	}

	.spread {
		display: grid;
		grid-template-columns: repeat(9, minmax(0, 1fr));
		gap: var(--space-2);
	}

	.spread li {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-2);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-bottom: 2px solid var(--class-color);
		border-radius: var(--radius-md);
	}

	.spread li.empty {
		opacity: 0.4;
		border-bottom-color: var(--grey-surface);
	}

	.spread img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.spread-count {
		font-weight: 700;
		font-variant-numeric: tabular-nums;
		color: var(--class-color);
	}

	@media (max-width: 40rem) {
		.spread {
			grid-template-columns: repeat(5, minmax(0, 1fr));
		}
	}

	.board {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--space-3);
		align-items: start;
	}

	@media (max-width: 64rem) {
		.board {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}

	@media (max-width: 34rem) {
		.board {
			grid-template-columns: 1fr;
		}
	}

	.column {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	.column-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding-bottom: var(--space-2);
		border-bottom: 1px solid var(--grey-surface);
	}

	.column-head h3 {
		font-size: var(--text-lg);
	}

	.column-count {
		margin-left: auto;
		font-size: var(--text-xl);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.column-empty {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.column-note {
		margin-top: calc(var(--space-1) * -1);
		color: var(--grey-text);
		font-size: var(--text-xs);
	}

	.players {
		display: flex;
		flex-direction: column;
	}

	.player {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--space-2) 0;
		border-top: 1px solid var(--grey-surface);
	}

	.player:first-child {
		border-top: 0;
		padding-top: 0;
	}

	.player-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 700;
	}

	.player-characters {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.entry {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		padding: 3px var(--space-1);
		border-radius: var(--radius-md);
	}

	.entry:hover {
		background-color: color-mix(in oklch, var(--class-color) 10%, transparent);
	}

	.entry.alt {
		opacity: 0.62;
	}

	.entry-specs {
		display: flex;
		flex: none;
		gap: 2px;
	}

	.entry-specs img {
		display: block;
		border-radius: 3px;
	}

	.entry-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--class-color);
		font-size: var(--text-md);
		font-weight: 600;
	}

	.alt-tag {
		flex: none;
		color: var(--grey-text);
		font-size: var(--text-xs);
	}

	.alt-tag {
		padding: 0 4px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.undecided {
		font-size: var(--text-sm);
	}
</style>
