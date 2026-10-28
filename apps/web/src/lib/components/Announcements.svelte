<script lang="ts">
	import type { Announcement } from '$lib/types';

	// `fill` takes the width it is given (a column of the home page) rather than the front page's
	// centred measure.
	let { announcements, fill = false }: { announcements: Announcement[]; fill?: boolean } = $props();

	const dateFormat = new Intl.DateTimeFormat(undefined, {
		month: 'short',
		day: 'numeric',
		year: 'numeric'
	});
</script>

<!-- The latest posts in the Discord server's #public-announcements, which the bot mirrors here. -->
<section aria-labelledby="announcements-heading" class:fill>
	<h2 id="announcements-heading">Announcements</h2>
	{#each announcements as post (post.id)}
		<article>
			<header>
				<span class="author">{post.author}</span>
				<time datetime={post.posted_at}>{dateFormat.format(new Date(post.posted_at))}</time>
				{#if post.edited_at}<span class="edited">(edited)</span>{/if}
			</header>
			<!-- The backend renders the post from Discord's Markdown and escapes all of it, so it is
			     safe to put on the page as HTML. -->
			<!-- eslint-disable-next-line svelte/no-at-html-tags -->
			<div class="post">{@html post.html}</div>
		</article>
	{/each}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		width: min(100% - 2rem, 40rem);
		padding-bottom: var(--space-6);
		text-align: left;
	}

	section.fill {
		width: 100%;
		padding-bottom: 0;
	}

	/* A rule under the heading, to set the feed off from the sign-in above it. */
	h2 {
		margin: 0;
		padding-bottom: var(--space-2);
		font-size: var(--text-lg);
		border-bottom: 1px solid var(--grey-surface);
	}

	article {
		padding: var(--space-3) var(--space-4);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	header {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.author {
		color: var(--foreground);
		font-weight: 600;
	}

	.post {
		line-height: var(--leading-normal);
		overflow-wrap: anywhere;
	}

	/* The rendered post's own markup (see the backend's `discord::markdown`). */
	.post :global(:is(p, h3, h4, h5, ul, ol, blockquote, pre)) {
		margin: var(--space-2) 0 0;
	}

	.post :global(h3) {
		font-size: var(--text-lg);
	}

	.post :global(:is(h4, h5)) {
		font-size: var(--text-md);
	}

	.post :global(small) {
		color: var(--grey-text);
		font-size: var(--text-xs);
	}

	.post :global(blockquote) {
		padding-left: var(--space-3);
		border-left: 3px solid var(--grey-soft);
	}

	.post :global(blockquote > :first-child) {
		margin-top: 0;
	}

	.post :global(:is(code, pre)) {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		background-color: var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.post :global(code) {
		padding: 0 0.25em;
	}

	.post :global(pre) {
		padding: var(--space-2) var(--space-3);
		overflow-x: auto;
	}

	.post :global(pre code) {
		padding: 0;
		background: none;
	}

	.post :global(a) {
		color: var(--accent-text);
	}

	.post :global(.mention) {
		padding: 0 0.2em;
		color: var(--accent-text);
		background-color: var(--accent-bg);
		border-radius: var(--radius-sm);
	}

	/* A spoiler stays hidden until pointed at or pressed. */
	.post :global(.spoiler) {
		color: transparent;
		background-color: var(--grey-soft);
		border-radius: var(--radius-sm);
		transition: color 0.1s;
	}

	.post :global(.spoiler:is(:hover, :active)) {
		color: inherit;
		background-color: var(--grey-surface);
	}
</style>
