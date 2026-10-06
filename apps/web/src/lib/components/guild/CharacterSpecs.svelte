<script lang="ts">
	import { page } from '$app/state';
	import { specIcon } from '$lib/wow/icons';
	import type { CharacterSpec, WowClass } from '$lib/types';

	/**
	 * A character's specs as icons, the main first and underlined. With `playing` (the spec played
	 * that night), the others are dimmed.
	 */
	let {
		cls,
		specs,
		playing = null,
		size = 20
	}: { cls: string; specs: CharacterSpec[]; playing?: string | null; size?: number } = $props();

	const wowClass = $derived(
		(page.data.classes as WowClass[] | undefined)?.find((c) => c.slug === cls)
	);
	const name = (slug: string) => wowClass?.specs.find((s) => s.slug === slug)?.name ?? slug;
</script>

{#if specs.length > 0}
	<span class="specs">
		{#each specs as spec (spec.spec)}
			<img
				src={specIcon(cls, spec.spec)}
				alt={name(spec.spec)}
				title="{name(spec.spec)}{spec.is_main ? ' (main spec)' : ''}"
				width={size}
				height={size}
				class:main={spec.is_main}
				class:dim={playing !== null && playing !== spec.spec}
			/>
		{/each}
	</span>
{/if}

<style>
	.specs {
		display: inline-flex;
		gap: 3px;
		vertical-align: middle;
	}

	img {
		display: block;
		border-radius: 3px;
	}

	.main {
		box-shadow: 0 2px 0 0 var(--red-solid);
	}

	.dim {
		opacity: 0.3;
	}
</style>
