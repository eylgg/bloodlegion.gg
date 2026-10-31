<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import { RANKS, RANK_GROUP, RANK_LABEL, RAIDING_RANKS } from '$lib/guild';
	import { classIcon, raceIcon } from '$lib/wow/icons';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { Member, Rank } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let members = $state<Member[]>(data.members);
	let error = $state('');

	const byRank = $derived(
		RANKS.map((rank) => ({ rank, members: members.filter((m) => m.guild_rank === rank) })).filter(
			(group) => group.members.length > 0
		)
	);

	// Mains of the raiding roster by class: the guild's shape at a glance.
	const spread = $derived(
		data.classes.map((wowClass) => ({
			wowClass,
			count: members.filter(
				(m) =>
					RAIDING_RANKS.includes(m.guild_rank) &&
					m.characters.some((c) => c.is_main && c.class === wowClass.slug)
			).length
		}))
	);

	// A member's main's class color, for the edge of their card; grey without a main.
	const classColor = (cls: string | undefined) =>
		data.classes.find((c) => c.slug === cls)?.color ?? 'var(--grey-solid)';

	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug);

	// Mirrors the backend's `may_set_rank`, so the picker only appears where it can work.
	function canSetRank(member: Member): boolean {
		if (data.user.is_superuser) return true;
		if (!data.officer || member.id === data.user.id) return false;
		const officerRank = (r: Rank) => r === 'leader' || r === 'officer';
		return data.user.guild_rank === 'leader' || !officerRank(member.guild_rank);
	}

	function rankChoices(): Rank[] {
		if (data.user.is_superuser || data.user.guild_rank === 'leader') return RANKS;
		return RANKS.filter((r) => r !== 'leader' && r !== 'officer');
	}

	async function setRank(member: Member, rank: Rank) {
		error = '';
		try {
			await api.put(`/api/guild/members/${member.id}/rank`, { rank });
			members = members.map((m) => (m.id === member.id ? { ...m, guild_rank: rank } : m));
		} catch (err) {
			error = errorMessage(err, 'Changing the rank failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>Roster | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<h1>Roster</h1>
	</div>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

<ul class="spread" aria-label="Raiding mains by class">
	{#each spread as { wowClass, count } (wowClass.slug)}
		<li
			class:empty={count === 0}
			style:--class-color={wowClass.color}
			title="{wowClass.name}: {count}"
		>
			<img src={classIcon(wowClass.slug)} alt={wowClass.name} width="28" height="28" />
			<span>{count}</span>
		</li>
	{/each}
</ul>

{#each byRank as group (group.rank)}
	<section>
		<h2>{RANK_GROUP[group.rank]} <span class="count">{group.members.length}</span></h2>
		<ul class="members">
			{#each group.members as member (member.id)}
				{@const main = member.characters.find((c) => c.is_main)}
				{@const alts = member.characters.filter((c) => !c.is_main)}
				<li style:--class-color={classColor(main?.class)}>
					<div class="head">
						<span class="username">{member.username}</span>
						{#if canSetRank(member)}
							<select
								aria-label="Rank of {member.username}"
								value={member.guild_rank}
								onchange={(event) => setRank(member, event.currentTarget.value as Rank)}
							>
								{#each rankChoices() as rank (rank)}
									<option value={rank}>{RANK_LABEL[rank]}</option>
								{/each}
							</select>
						{/if}
					</div>
					{#if main}
						{@const race = findRace(main.race)}
						<div class="main">
							{#if race}
								<img
									src={raceIcon(race.slug, main.sex)}
									alt={race.name}
									title={race.name}
									width="18"
									height="18"
								/>
							{/if}
							<CharacterLink
								id={main.id}
								firstName={main.first_name}
								lastName={main.last_name}
								cls={main.class}
							/>
						</div>
					{:else}
						<p class="none">No characters yet</p>
					{/if}
					{#if alts.length > 0}
						<div class="alts">
							{#each alts as alt (alt.id)}
								<CharacterLink
									id={alt.id}
									firstName={alt.first_name}
									lastName={alt.last_name}
									cls={alt.class}
								/>
							{/each}
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{:else}
	<p class="muted">Nobody on the roster yet.</p>
{/each}

<style>
	.spread {
		display: grid;
		grid-template-columns: repeat(9, minmax(0, 1fr));
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.spread li {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-2);
		color: var(--class-color);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-bottom: 2px solid var(--class-color);
		border-radius: var(--radius-md);
	}

	.spread li.empty {
		opacity: 0.4;
	}

	.spread img {
		display: block;
		border-radius: var(--radius-sm);
	}

	@media (max-width: 40rem) {
		.spread {
			grid-template-columns: repeat(5, minmax(0, 1fr));
		}
	}

	.count {
		color: var(--grey-text);
		font-weight: 400;
	}

	.members {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.members li {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--class-color);
		border-radius: var(--radius-lg);
	}

	/* The person first: their name, with the rank picker beside it for officers. */
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-height: 1.75rem;
	}

	.username {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		color: var(--foreground);
		font-size: var(--text-lg);
		font-weight: 700;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Compact beside the name, not the full width forms give a select. */
	.head select {
		flex: none;
		width: auto;
		font-size: var(--text-sm);
	}

	/* The main's race icon, then its class icon and name: the icons alike. */
	.main {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.main img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.none {
		margin: 0;
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-style: italic;
	}

	/* One a line, smaller than the main: that is what marks them as alts. */
	.alts {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
		padding-top: var(--space-2);
		font-size: var(--text-sm);
		border-top: 1px solid var(--grey-surface);
	}
</style>
