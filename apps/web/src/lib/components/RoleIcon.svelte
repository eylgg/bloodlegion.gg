<script lang="ts">
	import type { Role } from '$lib/types';
	import { ROLE_LABEL } from '$lib/wow/icons';

	/**
	 * A role badge, drawn as vectors so it stays crisp at any size: the game's own role art is
	 * 16-19 pixel bitmaps. A white glyph on the conventional role color: blue shield for tank,
	 * green cross for healer, red sword for melee, orange crosshair for ranged.
	 */
	let {
		role,
		size = 20,
		decorative = false
	}: { role: Role; size?: number; decorative?: boolean } = $props();
</script>

<svg
	class="role role-{role}"
	viewBox="0 0 24 24"
	width={size}
	height={size}
	role={decorative ? undefined : 'img'}
	aria-hidden={decorative ? 'true' : undefined}
	aria-label={decorative ? undefined : ROLE_LABEL[role]}
>
	<circle cx="12" cy="12" r="11.25" class="badge" />
	{#if role === 'tank'}
		<path
			d="M12 4.6 L18 6.9 V11.4 C18 15.1 15.6 18.1 12 19.7 C8.4 18.1 6 15.1 6 11.4 V6.9 Z"
			class="glyph"
		/>
	{:else if role === 'healer'}
		<path
			d="M10.1 5.6 H13.9 V10.1 H18.4 V13.9 H13.9 V18.4 H10.1 V13.9 H5.6 V10.1 H10.1 Z"
			class="glyph"
		/>
	{:else if role === 'melee'}
		<path d="M18.6 5.4 L17.9 8.8 L10 16.7 L7.3 14 L15.2 6.1 Z" class="glyph" />
		<path d="M5.6 14.4 L6.9 13.1 L10.9 17.1 L9.6 18.4 Z" class="glyph" />
		<path d="M7.4 17.1 L8.4 18.1 L6.2 20.3 L5.2 19.3 Z" class="glyph" />
	{:else}
		<path
			fill-rule="evenodd"
			d="M12 5.8 a6.2 6.2 0 1 0 0 12.4 a6.2 6.2 0 1 0 0 -12.4 Z M12 7.9 a4.1 4.1 0 1 1 0 8.2 a4.1 4.1 0 1 1 0 -8.2 Z"
			class="glyph"
		/>
		<circle cx="12" cy="12" r="1.7" class="glyph" />
		<path
			d="M11.15 3.6 h1.7 v3.2 h-1.7 Z M11.15 17.2 h1.7 v3.2 h-1.7 Z M3.6 11.15 v1.7 h3.2 v-1.7 Z M17.2 11.15 v1.7 h3.2 v-1.7 Z"
			class="glyph"
		/>
	{/if}
</svg>

<style>
	.role {
		display: block;
		flex: none;
	}

	.badge {
		stroke: rgb(255 255 255 / 0.18);
		stroke-width: 1;
	}

	.role-tank .badge {
		fill: #2f6fc0;
	}

	.role-healer .badge {
		fill: #2e9b57;
	}

	.role-melee .badge {
		fill: #b8322b;
	}

	.role-ranged .badge {
		fill: #c06a1f;
	}

	.glyph {
		fill: #fff;
	}
</style>
