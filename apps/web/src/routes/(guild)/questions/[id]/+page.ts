import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { QuestionDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { detail: await api.get<QuestionDetail>(`/api/questions/${params.id}`, { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'That question could not be loaded.');
	}
};
