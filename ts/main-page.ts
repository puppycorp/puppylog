import { logsSearchPage } from "./logs"
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility";

export const mainPage = (root: HTMLElement) => {
	let query: string | undefined = getQueryParam("query") || ""
	let isStreaming = getQueryParam("stream") === "true"
	logsSearchPage({
		root,
		streamLogs: (args, onNewLog, onEnd) => {
			const streamQuery = new URLSearchParams()
			if (args.query) streamQuery.append("query", args.query)
			if (args.count) streamQuery.append("count", args.count.toString())
			if (args.endDate) streamQuery.append("endDate", args.endDate)
			const streamUrl = new URL("/api/logs", window.location.origin)
			streamUrl.search = streamQuery.toString()
			const eventSource = new EventSource(streamUrl)
			eventSource.onmessage = (event) => {
				const data = JSON.parse(event.data)
				onNewLog(data)
			}
			eventSource.onerror = (event) => {
				console.log("eventSource.onerror", event)
				eventSource.close()
				onEnd()
			}
			return () => eventSource.close()
		},
		validateQuery: async (query) => {
			let res = await fetch(`/api/v1/validate_query?query=${encodeURIComponent(query)}`)
			if (res.status === 200) return null
			return res.text()
		}
	})
	return root
}