<script lang="ts">
	import { page } from '$app/state';
	import { specIcon } from '$lib/wow/icons';
	import type { WowClass } from '$lib/types';

	/** A character's specs as icons, the main spec first; the one played that night, if given, lit. */
	let {
		cls,
		primary,
		secondary = null,
		playing = null,
		size = 20
	}: {
		cls: string;
		primary: string | null;
		secondary?: string | null;
		playing?: string | null;
		size?: number;
	} = $props();

	const wowClass = $derived(
		(page.data.classes as WowClass[] | undefined)?.find((c) => c.slug === cls)
	);
	const name = (slug: string) => wowClass?.specs.find((s) => s.slug === slug)?.name ?? slug;
	const specs = $derived([primary, secondary].filter((s): s is string => s !== null));
</script>

{#if specs.length > 0}
	<span class="specs">
		{#each specs as spec, i (i)}
			<img
				src={specIcon(cls, spec)}
				alt={name(spec)}
				title="{name(spec)}{i === 0 ? ' (main spec)' : ' (second spec)'}"
				width={size}
				height={size}
				class:dim={playing !== null && playing !== spec}
			/>
		{/each}
	</span>
{/if}

<style>
	.specs {
		display: inline-flex;
		gap: 2px;
		vertical-align: middle;
	}

	img {
		display: block;
		border-radius: 3px;
	}

	.dim {
		opacity: 0.3;
	}
</style>
