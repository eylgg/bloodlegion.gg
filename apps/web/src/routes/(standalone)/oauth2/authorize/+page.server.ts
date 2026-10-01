import { error, redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { api, statusFrom } from '$lib/api';

type AuthorizeResponse =
	| { action: 'redirect'; url: string }
	| { action: 'consent'; client_name: string; scopes: { name: string; description: string }[] };

export const load: PageServerLoad = async ({ url, fetch }) => {
	let response: AuthorizeResponse;
	try {
		response = await api.get<AuthorizeResponse>(`/api/auth/oauth2/authorize${url.search}`, {
			fetch
		});
	} catch (err) {
		error(statusFrom(err));
	}

	if (response.action === 'redirect') {
		redirect(303, response.url);
	}

	return {
		clientName: response.client_name,
		scopes: response.scopes,
		query: url.search
	};
};
