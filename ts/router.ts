import { patternMatcher } from "./pattern-matcher"

export const routes = (routes: any, container: HTMLElement) => {
	const matcher = patternMatcher(routes)

	const handleRoute = (path: string) => {
		const result = matcher.match(path)
		console.log("match result", result)
		container.innerHTML = ""
		if (!result) {
			const notFound = document.createElement("div")
			notFound.innerHTML = "Not found"
			container.appendChild(notFound)
			return notFound
		}
		container.appendChild(result.result)
	}

	handleRoute(window.location.pathname)
	window.addEventListener('popstate', () => {
		handleRoute(window.location.pathname);
	})
	return (path: string) => {
		window.history.pushState({}, '', path)
		handleRoute(path)
	}
}