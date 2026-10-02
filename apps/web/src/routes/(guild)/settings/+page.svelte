<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import { formatInZone, zoneCity } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { GuildSettings } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let timeZone = $state(data.settings.time_zone);
	// svelte-ignore state_referenced_locally
	let defaultRaidTime = $state(data.settings.default_raid_time);
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
			await api.put<GuildSettings>('/api/guild/settings', {
				time_zone: timeZone,
				default_raid_time: defaultRaidTime
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
	<title>Settings | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Admin</p>
		<h1>Settings</h1>
	</div>
</div>

<form class="panel" onsubmit={save}>
	<h2>Raid times</h2>
	<p class="muted">
		Raids are scheduled on the guild's clock. A raid keeps the zone it was scheduled in, so changing
		the zone here moves only raids scheduled from now on. Members see every raid in its own zone.
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
	<p class="muted">
		It is {formatInZone(new Date().toISOString(), timeZone)} in {zoneCity(timeZone)} now.
	</p>
	<div class="form-actions">
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : 'Save'}
		</Button>
	</div>
</form>

<style>
	.panel {
		max-width: 44rem;
	}
</style>
