import { logsSearchPage } from "./logs"
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility";

export const mainPage = (root: HTMLElement) => {
	let query: string | undefined = getQueryParam("query") || ""
	let isStreaming = getQueryParam("stream") === "true"
	logsSearchPage({
		root,
		streamLogs: (query, onNewLog, onEnd) => {
			const streamQuery = new URLSearchParams()
			if (query) streamQuery.append("query", query)
			const streamUrl = new URL("/api/logs/stream", window.location.origin)
			streamUrl.search = streamQuery.toString()
			const eventSource = new EventSource(streamUrl)
			eventSource.onmessage = (event) => {
				const data = JSON.parse(event.data)
				onNewLog(data)
			}
			eventSource.onerror = (event) => {
				eventSource.close()
				onEnd()
			}
			return () => eventSource.close()
		},
		fetchMore: async (args) => {
			query = args.query
			if (query) setQueryParam("query", query)
			else removeQueryParam("query")
			const urlQuery = new URLSearchParams()
			const offsetInMinutes = new Date().getTimezoneOffset();
			const offsetInHours = -offsetInMinutes / 60;
			urlQuery.append("timezone", offsetInHours.toString())
			if (args.query) urlQuery.append("query", args.query)
			if (args.count) urlQuery.append("count", args.count.toString())
			if (args.endDate) urlQuery.append("endDate", args.endDate)
			const url = new URL("/api/logs", window.location.origin)
			url.search = urlQuery.toString()
			const res = await fetch(url.toString())
			if (res.status === 400) {
				const err = await res.json()
				console.error("res.error", err)
				throw new Error(err.error)
			} else if (res.status !== 200) {
				const text = await res.text()
				throw new Error(text)
			}
			return res.json()
		}
	})
	return root
}