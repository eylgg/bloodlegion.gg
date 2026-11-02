import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import type { PageLoad } from './$types';

// The characters are the roster now.
export const load: PageLoad = () => {
	redirect(308, resolve('/roster'));
};
