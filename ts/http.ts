import { getIdToken, onAuthStateChange, signOut } from "./auth"

let cachedToken: string | null = getIdToken()

onAuthStateChange((user) => {
	cachedToken = user?.token ?? null
})

const buildHeaders = (init?: RequestInit): Headers => {
	const headers = new Headers(init?.headers || undefined)
	if (cachedToken) {
		headers.set("Authorization", `Bearer ${cachedToken}`)
	}
	return headers
}

export const apiFetch = async (
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> => {
	const response = await fetch(input, {
		...init,
		headers: buildHeaders(init),
	})
	if (response.status === 401 && cachedToken) {
		signOut()
	}
	return response
}

export const withAuthQuery = (url: URL): URL => {
	if (cachedToken) {
		url.searchParams.set("token", cachedToken)
	}
	return url
}
