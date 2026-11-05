<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { FACTION_LABEL, RULESETS, formatInZone, zoneCity } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import DiscordSettings from '$lib/components/guild/DiscordSettings.svelte';
	import type { GuildSettings } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let timeZone = $state(data.settings.time_zone);
	// svelte-ignore state_referenced_locally
	let defaultRaidTime = $state(data.settings.default_raid_time);
	// svelte-ignore state_referenced_locally
	let faction = $state(data.settings.faction);
	// svelte-ignore state_referenced_locally
	let ruleset = $state(data.settings.ruleset);
	let saving = $state(false);
	let error = $state('');
	let saved = $state(false);

	// Every zone the browser knows, the current one included even if it does not.
	const zones = $derived.by(() => {
		const known = Intl.supportedValuesOf('timeZone');
		return known.includes(data.settings.time_zone) ? known : [data.settings.time_zone, ...known];
	});

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = '';
		saved = false;
		try {
			await api.put<GuildSettings>('/api/guild', {
				time_zone: timeZone,
				default_raid_time: defaultRaidTime,
				faction,
				ruleset
			});
			// Every page under the guild layout reads the settings from its load.
			await invalidateAll();
			saved = true;
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>Administration | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Administration</h1>
	</div>
</div>

<form class="panel" onsubmit={save}>
	<h2>The guild</h2>
	<p class="muted">
		Raids are scheduled on the guild's clock. A raid keeps the zone it was scheduled in, so changing
		the zone here moves only raids scheduled from now on. Members see every raid in its own zone.
		New characters take the guild's ruleset, and its faction's races come first when picking one.
	</p>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	{#if saved}<Alert variant="success">Saved.</Alert>{/if}
	<div class="row">
		<label class="field">
			Time zone
			<select bind:value={timeZone}>
				{#each zones as zone (zone)}
					<option value={zone}>{zone.replace(/_/g, ' ')}</option>
				{/each}
			</select>
		</label>
		<label class="field">
			Raids usually start at
			<input type="time" bind:value={defaultRaidTime} required />
		</label>
	</div>
	<div class="row">
		<label class="field">
			Faction
			<select bind:value={faction}>
				{#each Object.entries(FACTION_LABEL) as [value, label] (value)}
					<option {value}>{label}</option>
				{/each}
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
	</div>
	<p class="muted">
		It is {formatInZone(new Date().toISOString(), timeZone)} in {zoneCity(timeZone)} now.
	</p>
	<div class="form-actions">
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : 'Save'}
		</Button>
	</div>
</form>

<DiscordSettings settings={data.discord} />

<section class="panel">
	<div class="head">
		<h2>Roster uploads</h2>
		<Button href={resolve('/admin/rosters')} variant="secondary" size="small">View</Button>
	</div>
	<p class="muted">
		Every roster the officers' Blood Legion clients sent from the addon: who sent it, the levels it
		changed, and an undo for one with bad data.
	</p>
</section>

<section class="panel">
	<div class="head">
		<h2>Battle.net characters</h2>
		<Button href={resolve('/admin/battlenet')} variant="secondary" size="small">View</Button>
	</div>
	<p class="muted">
		The Forever characters on members' Battle.net accounts, read at each sign-in: those already on
		the site, and the hidden rest.
	</p>
</section>

<style>
	.panel {
		max-width: 44rem;
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}
</style>
