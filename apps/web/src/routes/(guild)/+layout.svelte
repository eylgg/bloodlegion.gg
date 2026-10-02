<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { api } from '$lib/api';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();
	let signingOut = $state(false);

	const links = $derived([
		{ href: resolve('/roster'), label: 'Roster' },
		{ href: resolve('/raids'), label: 'Raids' },
		{ href: resolve('/loot'), label: 'Loot' },
		{ href: resolve('/bosses'), label: 'Bosses' },
		{ href: resolve('/items'), label: 'Items' },
		{ href: resolve('/characters'), label: 'Characters' },
		{ href: resolve('/questions'), label: 'Questions' },
		...(data.officer ? [{ href: resolve('/notes'), label: 'Notes' }] : [])
	]);

	const isActive = (href: string) =>
		page.url.pathname === href || page.url.pathname.startsWith(href + '/');

	async function signOut() {
		signingOut = true;
		try {
			await api.post('/api/auth/logout');
		} finally {
			window.location.assign('/');
		}
	}
</script>

<div class="shell">
	<header>
		<a href={resolve('/')} class="brand">
			<img src="/wordmark.svg" alt="Blood Legion" width="659" height="201" />
		</a>
		<nav aria-label="Guild">
			{#each links as link (link.href)}
				<a href={link.href} aria-current={isActive(link.href) ? 'page' : undefined}>{link.label}</a>
			{/each}
		</nav>
		<div class="account">
			<a
				href={resolve('/profile')}
				aria-current={isActive(resolve('/profile')) ? 'page' : undefined}>{data.user.username}</a
			>
			<button type="button" onclick={signOut} disabled={signingOut}>Sign out</button>
		</div>
	</header>
	<main>
		{@render children()}
	</main>
</div>

<style>
	.shell {
		min-height: 100vh;
		min-height: 100dvh;
		background: radial-gradient(ellipse 80% 30rem at 50% -12rem, var(--red-glow), transparent);
	}

	header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-6);
		max-width: 72rem;
		margin: 0 auto;
		padding: var(--space-4) var(--space-4) 0;
	}

	.brand img {
		display: block;
		width: 7.5rem;
		height: auto;
	}

	nav {
		display: flex;
		flex: 1;
		gap: var(--space-1);
		/* On a phone the links scroll sideways rather than wrapping into a wall. */
		overflow-x: auto;
		scrollbar-width: none;
	}

	nav a,
	.account a {
		padding: var(--space-1) var(--space-2);
		color: var(--grey-text);
		font-size: var(--text-md);
		font-weight: 500;
		text-decoration: none;
		white-space: nowrap;
		border-radius: var(--radius-md);
	}

	nav a:hover,
	.account a:hover {
		color: var(--foreground);
		background-color: var(--grey-bg);
	}

	nav a[aria-current='page'],
	.account a[aria-current='page'] {
		color: var(--foreground);
		background-color: var(--grey-surface);
	}

	.account {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
	}

	.account button {
		padding: 0;
		color: var(--grey-text);
		font: inherit;
		background: none;
		border: 0;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	.account button:hover {
		color: var(--grey-text-active);
	}

	@media (max-width: 48rem) {
		nav {
			order: 3;
			flex-basis: 100%;
		}

		.account {
			margin-left: auto;
		}
	}

	main {
		display: flex;
		flex-direction: column;
		gap: var(--space-8);
		max-width: 72rem;
		margin: 0 auto;
		padding: var(--space-8) var(--space-4) calc(var(--space-8) * 2);
	}

	/* Shared by every page under here: headings, sections, and plain form controls. */
	main :global(h1) {
		font-size: clamp(1.75rem, 5vw, 2.25rem);
		line-height: 1.1;
	}

	main :global(h2) {
		font-size: var(--text-xl);
	}

	main :global(.page-head) {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		justify-content: space-between;
		gap: var(--space-3);
	}

	main :global(.kicker) {
		color: var(--red-text);
		font-size: var(--text-sm);
		font-weight: 600;
		letter-spacing: 0.12em;
		text-transform: uppercase;
	}

	main :global(.muted) {
		color: var(--grey-text);
		font-size: var(--text-md);
	}

	main :global(section) {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	main :global(.panel) {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-6);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-xl);
	}

	main :global(.row) {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: var(--space-3);
	}

	main :global(.row > *) {
		flex: 1 1 10rem;
	}

	main :global(.row > .fit) {
		flex: 0 0 auto;
	}

	main :global(label.field) {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--grey-text);
	}

	main :global(select),
	main :global(textarea),
	main :global(input[type='datetime-local']),
	main :global(input[type='search']),
	main :global(input[type='text']),
	main :global(input[type='number']) {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		color: var(--foreground);
		font: inherit;
		font-size: var(--text-md);
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
	}

	main :global(select:focus),
	main :global(textarea:focus),
	main :global(input:focus) {
		outline: none;
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 3px var(--accent-bg);
	}

	main :global(textarea) {
		min-height: 8rem;
		resize: vertical;
	}

	main :global(input[type='checkbox']) {
		accent-color: var(--accent-solid);
	}

	main :global(.form-actions) {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
	}
</style>
