<script lang="ts">
	import { spellIcon, type Spell } from '$lib/wow/spells';

	/** A spell's tooltip at its highest rank, drawn the way the game draws it. */
	let { spell }: { spell: Spell } = $props();
</script>

<div class="tooltip">
	<img class="icon" src={spellIcon(spell.icon)} alt="" width="40" height="40" />
	<div class="body">
		<p class="split">
			<span class="name">{spell.name}</span>
			{#if spell.rank}<span class="muted">{spell.rank}</span>{/if}
		</p>
		{#each spell.tooltip as line, i (i)}
			{#if typeof line === 'string'}
				<p>{line}</p>
			{:else}
				<p class="split"><span>{line[0]}</span><span>{line[1]}</span></p>
			{/if}
		{/each}
		{#each spell.description as line, i (i)}
			<p class="gold">{line}</p>
		{/each}
		{#if spell.talent}<p class="muted note">Talent</p>{/if}
	</div>
</div>

<style>
	/* The game's own tooltip look, as `ItemTooltip` draws it. */
	.tooltip {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		max-width: 22rem;
		color: #fff;
		font-size: 0.8125rem;
		line-height: 1.35;
		text-align: left;
	}

	.icon {
		flex: none;
		border: 1px solid #555;
		border-radius: 4px;
	}

	.body {
		min-width: 12rem;
		padding: var(--space-2) var(--space-3);
		background-color: rgb(9 11 31 / 0.96);
		border: 1px solid #5a6177;
		border-radius: 4px;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.5);
	}

	p {
		margin: 0;
	}

	.name {
		font-size: 0.9375rem;
		font-weight: 600;
	}

	.split {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: var(--space-4);
	}

	.gold {
		color: #ffd100;
	}

	.muted {
		color: #9d9d9d;
	}

	.note {
		margin-top: var(--space-1);
		font-size: 0.75rem;
	}
</style>
