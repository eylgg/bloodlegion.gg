<script lang="ts">
	import { page } from '$app/state';
	import Logo from '$lib/components/Logo.svelte';
</script>

<svelte:head>
	<title>Blood Legion</title>
</svelte:head>

<!-- Every path shows the mark; the status is kept, quietly, so a bad link is still recognisable. -->
<Logo />
<p class="status">{page.status}: {page.error?.message}</p>

<style>
	.status {
		position: fixed;
		inset: auto 0 var(--space-6);
		margin: 0;
		text-align: center;
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
	}
</style>
