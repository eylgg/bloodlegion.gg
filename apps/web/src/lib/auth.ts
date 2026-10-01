import { redirect } from '@sveltejs/kit';
import type { User } from '$lib/types';

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
