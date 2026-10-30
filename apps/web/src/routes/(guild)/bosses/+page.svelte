<script lang="ts">
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Bosses | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<h1>Bosses</h1>
	</div>
</div>

<p class="muted">
	Each raid's bosses, in encounter order. A boss's page shows what it has dropped and how often.
</p>

<div class="zones">
	{#each data.zones as zone (zone.slug)}
		{@const zoneBosses = data.bosses.filter((b) => b.zone === zone.slug)}
		<section class="panel">
			<h2>{zone.name} <span class="muted">{zone.size} players</span></h2>
			<ol>
				{#each zoneBosses as boss (boss.id)}
					<li>
						<a href={resolve('/(guild)/bosses/[id]', { id: String(boss.id) })}>{boss.name}</a>
					</li>
				{:else}
					<li class="muted">No bosses yet.</li>
				{/each}
			</ol>
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
</style>
