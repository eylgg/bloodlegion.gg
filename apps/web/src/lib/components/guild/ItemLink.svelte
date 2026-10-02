<script lang="ts">
	import { resolve } from '$app/paths';
	import { QUALITY_COLOR } from '$lib/guild';
	import type { Quality } from '$lib/types';

	let { id, name, quality }: { id: number; name: string; quality: Quality } = $props();
</script>

<a
	class="item"
	href={resolve('/(guild)/items/[id]', { id: String(id) })}
	style:color={QUALITY_COLOR[quality]}>[{name}]</a
>

<style>
	.item {
		font-weight: 600;
		text-decoration: none;
	}

	.item:hover {
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}
</style>
