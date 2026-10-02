<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import { RANKS, RANK_LABEL, RAIDING_RANKS } from '$lib/guild';
	import { classIcon } from '$lib/wow/icons';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { Member, Rank } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let members = $state<Member[]>(data.members);
	let everyone = $state(false);
	let error = $state('');

	const shown = $derived(
		everyone ? members : members.filter((m) => RAIDING_RANKS.includes(m.guild_rank))
	);
	const byRank = $derived(
		RANKS.map((rank) => ({ rank, members: shown.filter((m) => m.guild_rank === rank) })).filter(
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
		<p class="kicker">The guild</p>
		<h1>Roster</h1>
	</div>
	<label class="toggle">
		<input type="checkbox" bind:checked={everyone} />
		Everyone, not just the raiding roster
	</label>
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
		<h2>{RANK_LABEL[group.rank]}s <span class="count">{group.members.length}</span></h2>
		<ul class="members">
			{#each group.members as member (member.id)}
				{@const main = member.characters.find((c) => c.is_main)}
				{@const alts = member.characters.filter((c) => !c.is_main)}
				<li>
					<div class="who">
						{#if main}
							<CharacterLink
								id={main.id}
								firstName={main.first_name}
								lastName={main.last_name}
								cls={main.class}
							/>
						{:else}
							<span class="muted">No main yet</span>
						{/if}
						<span class="username">{member.username}</span>
					</div>
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
				</li>
			{/each}
		</ul>
	</section>
{:else}
	<p class="muted">Nobody on the roster yet.</p>
{/each}

<style>
	.toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
		cursor: pointer;
	}

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
		border-radius: var(--radius-lg);
	}

	.who {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.username {
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.alts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-3);
		font-size: var(--text-sm);
		opacity: 0.75;
	}
</style>
