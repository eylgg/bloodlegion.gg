<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { classIcon } from '$lib/wow/icons';
	import type { WowClass } from '$lib/types';

	// The guild layout loads the class catalog for every page under it.
	let {
		id,
		firstName,
		lastName,
		cls,
		icon = true
	}: { id: number; firstName: string; lastName: string; cls: string; icon?: boolean } = $props();

	const color = $derived(
		(page.data.classes as WowClass[] | undefined)?.find((c) => c.slug === cls)?.color
	);
</script>

<a
	class="character"
	href={resolve('/(guild)/characters/[id]', { id: String(id) })}
	style:--class-color={color ?? 'var(--foreground)'}
>
	{#if icon}<img src={classIcon(cls)} alt="" width="18" height="18" />{/if}
	<span>{firstName} {lastName}</span>
</a>

<style>
	.character {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--class-color);
		font-weight: 600;
		text-decoration: none;
		white-space: nowrap;
	}

	.character:hover span {
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	img {
		display: block;
		border-radius: var(--radius-sm);
	}
</style>
