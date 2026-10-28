<script lang="ts">
	import GuildShell from '$lib/components/guild/GuildShell.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();
</script>

<GuildShell user={data.user}>
	{@render children()}
</GuildShell>
