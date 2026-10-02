<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { QUALITIES, QUALITY_LABEL } from '$lib/guild';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const value = (key: string) => page.url.searchParams.get(key) ?? '';
	const zone = $derived(value('zone'));
	const bosses = $derived(zone ? data.bosses.filter((b) => b.zone === zone) : data.bosses);

	// Each change is a navigation, so the URL is the state and the load refetches.
	function set(key: string, next: string) {
		// A boss belongs to one zone; changing the zone drops it.
		const kept = [...page.url.searchParams].filter(
			([k]) => k !== key && !(key === 'zone' && k === 'boss_id')
		);
		const search = new URLSearchParams(next ? [...kept, [key, next]] : kept).toString();
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${page.url.pathname}${search ? `?${search}` : ''}`, {
			keepFocus: true,
			noScroll: true,
			replaceState: true
		});
	}
</script>

<svelte:head>
	<title>Loot | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Loot</h1>
	</div>
</div>

<div class="row">
	<label class="field">
		Raid
		<select value={zone} onchange={(e) => set('zone', e.currentTarget.value)}>
			<option value="">Every raid</option>
			{#each data.zones as z (z.slug)}
				<option value={z.slug}>{z.name}</option>
			{/each}
		</select>
	</label>
	<label class="field">
		Boss
		<select value={value('boss_id')} onchange={(e) => set('boss_id', e.currentTarget.value)}>
			<option value="">Every boss</option>
			{#each bosses as boss (boss.id)}
				<option value={String(boss.id)}>{boss.name}</option>
			{/each}
		</select>
	</label>
	<label class="field">
		Class
		<select value={value('class')} onchange={(e) => set('class', e.currentTarget.value)}>
			<option value="">Every class</option>
			{#each data.classes as wowClass (wowClass.slug)}
				<option value={wowClass.slug}>{wowClass.name}</option>
			{/each}
		</select>
	</label>
	<label class="field">
		Quality
		<select value={value('quality')} onchange={(e) => set('quality', e.currentTarget.value)}>
			<option value="">Any quality</option>
			{#each QUALITIES as quality (quality)}
				<option value={quality}>{QUALITY_LABEL[quality]}</option>
			{/each}
		</select>
	</label>
</div>

<p class="muted">
	{data.loot.length}
	{data.loot.length === 1 ? 'item' : 'items'}{data.loot.length >= 500 ? ' (the latest 500)' : ''}
</p>

<LootTable loot={data.loot} empty="No loot matches." />
