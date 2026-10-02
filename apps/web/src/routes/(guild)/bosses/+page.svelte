<script lang="ts">
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { Boss } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let bosses = $state<Boss[]>(data.bosses);
	let error = $state('');
	// The name being typed for each zone's new boss.
	let drafts = $state<Record<string, string>>({});
	let renaming = $state<number | null>(null);
	let renameDraft = $state('');

	async function add(event: SubmitEvent, zone: string) {
		event.preventDefault();
		error = '';
		try {
			const boss = await api.post<Boss>('/api/bosses', { zone, name: drafts[zone] ?? '' });
			bosses = [...bosses, boss];
			drafts[zone] = '';
		} catch (err) {
			error = errorMessage(err, 'Adding the boss failed. Please try again.');
		}
	}

	async function rename(event: SubmitEvent, boss: Boss) {
		event.preventDefault();
		error = '';
		try {
			const saved = await api.put<Boss>(`/api/bosses/${boss.id}`, { name: renameDraft });
			bosses = bosses.map((b) => (b.id === saved.id ? saved : b));
			renaming = null;
		} catch (err) {
			error = errorMessage(err, 'Renaming failed. Please try again.');
		}
	}

	async function remove(boss: Boss) {
		if (!confirm(`Remove ${boss.name}?`)) return;
		error = '';
		try {
			await api.del(`/api/bosses/${boss.id}`);
			bosses = bosses.filter((b) => b.id !== boss.id);
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>Bosses | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Bosses</h1>
	</div>
</div>

<p class="muted">
	WoW: Forever's raids are new, so officers add each boss as the guild meets it. A boss's page shows
	what it has dropped and how often.
</p>

{#if error}<Alert variant="error">{error}</Alert>{/if}

<div class="zones">
	{#each data.zones as zone (zone.slug)}
		{@const zoneBosses = bosses.filter((b) => b.zone === zone.slug)}
		<section class="panel">
			<h2>{zone.name} <span class="muted">{zone.size} players</span></h2>
			<ol>
				{#each zoneBosses as boss (boss.id)}
					<li>
						{#if renaming === boss.id}
							<form class="inline" onsubmit={(e) => rename(e, boss)}>
								<input type="text" bind:value={renameDraft} maxlength="64" aria-label="Name" />
								<Button type="submit" size="small" variant="primary">Save</Button>
								<Button size="small" variant="secondary" onclick={() => (renaming = null)}>
									Cancel
								</Button>
							</form>
						{:else}
							<a href={resolve('/(guild)/bosses/[id]', { id: String(boss.id) })}>{boss.name}</a>
							{#if data.officer}
								<span class="actions">
									<Button
										size="small"
										variant="secondary"
										onclick={() => {
											renaming = boss.id;
											renameDraft = boss.name;
										}}>Rename</Button
									>
									<Button size="small" variant="danger" onclick={() => remove(boss)}>Remove</Button>
								</span>
							{/if}
						{/if}
					</li>
				{:else}
					<li class="muted">No bosses entered yet.</li>
				{/each}
			</ol>
			{#if data.officer}
				<form class="inline" onsubmit={(e) => add(e, zone.slug)}>
					<input
						type="text"
						bind:value={drafts[zone.slug]}
						maxlength="64"
						placeholder="Add a boss"
						aria-label="New boss in {zone.name}"
						required
					/>
					<Button type="submit" size="small" variant="primary">Add</Button>
				</form>
			{/if}
		</section>
	{/each}
</div>

<style>
	.zones {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
		gap: var(--space-3);
		align-items: start;
	}

	h2 .muted {
		font-size: var(--text-sm);
		font-weight: 400;
	}

	ol {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding-left: var(--space-6);
	}

	li {
		padding-left: var(--space-1);
	}

	li.muted {
		list-style: none;
		margin-left: calc(var(--space-6) * -1);
	}

	li a {
		color: var(--foreground);
		font-weight: 600;
	}

	.actions {
		display: inline-flex;
		gap: var(--space-1);
		margin-left: var(--space-2);
	}

	.inline {
		display: flex;
		gap: var(--space-2);
	}
</style>
