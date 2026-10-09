<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	// The week carries over between the two plans.
	const week = $derived(page.url.searchParams.get('week'));
	const tabs = $derived(
		[
			{ href: resolve('/planner'), label: 'Raids' },
			{ href: resolve('/planner/loot'), label: 'Loot' }
		].map((tab) => ({
			...tab,
			current: page.url.pathname === tab.href,
			link: week ? `${tab.href}?week=${encodeURIComponent(week)}` : tab.href
		}))
	);
</script>

<nav class="tabs" aria-label="Planner">
	{#each tabs as tab (tab.href)}
		<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- resolved above, plus the week -->
		<a href={tab.link} aria-current={tab.current ? 'page' : undefined}>{tab.label}</a>
	{/each}
</nav>

{@render children()}

<style>
	.tabs {
		display: flex;
		gap: var(--space-1);
		border-bottom: 1px solid var(--grey-surface);
	}

	a {
		margin-bottom: -1px;
		padding: var(--space-2) var(--space-4);
		color: var(--grey-text);
		font-weight: 600;
		text-decoration: none;
		border-bottom: 2px solid transparent;
	}

	a:hover {
		color: var(--foreground);
	}

	a[aria-current='page'] {
		color: var(--foreground);
		border-bottom-color: var(--accent-solid);
	}
</style>
