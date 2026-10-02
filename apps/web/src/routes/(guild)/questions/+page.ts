import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Question } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		return { questions: await api.get<Question[]>('/api/questions', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The questions are unavailable right now.');
	}
};
