import { redirect } from '@sveltejs/kit';
import type { LoginProvider, User } from '$lib/types';

/**
 * The default post-auth redirect target. Must match `NEXT_FALLBACK` in
 * crates/bloodlegion/src/auth/next.rs, where an absent or unsafe `next` falls back to this path.
 */
export const NEXT_FALLBACK = '/';

/** Asserts a session, redirecting to the front page to sign in (and back here afterwards). */
export function requireUser(user: User | null, url: URL): asserts user is User {
	if (user) return;
	const next = url.pathname + url.search;
	redirect(303, next === NEXT_FALLBACK ? '/' : `/?next=${encodeURIComponent(next)}`);
}

/**
 * The button style for a provider's sign-in and link buttons: its own brand colour for the ones
 * that have one, the house style for any other.
 */
export function providerVariant(slug: string): 'battlenet' | 'discord' | 'secondary' {
	return slug === 'battlenet' || slug === 'discord' ? slug : 'secondary';
}

/**
 * The name of the provider a sign-in or link came back from: the backend adds `?provider=<slug>`
 * when it knows which one it was. Looked up among the listed providers, so a crafted link cannot
 * put its own words on the page; `null` when absent or unknown.
 */
export function returnedProviderName(providers: LoginProvider[], url: URL): string | null {
	const slug = url.searchParams.get('provider');
	const provider = providers.find((p) => p.kind === 'oauth2' && p.slug === slug);
	return provider?.kind === 'oauth2' ? provider.name : null;
}
