import { LogEntry, logsSearchPage } from "./logs"
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility";

export const mainPage = () => {
	let query = getQueryParam("query") || ""
	let logEventSource: EventSource | null = null  
	let isStreaming = getQueryParam("stream") === "true"
	const startStream = (query: string) => {
		if (logEventSource) logEventSource.close()
		logEventSource = null
		const streamQuery = new URLSearchParams()
		if (query) streamQuery.append("query", query)
		const streamUrl = new URL("/api/logs/stream", window.location.origin)
		streamUrl.search = streamQuery.toString()
		logEventSource = new EventSource(streamUrl)
		logEventSource.onmessage = (event) => {
			const data = JSON.parse(event.data)
			addLogEntries([data])
		}
		logEventSource.onerror = (event) => {
			console.error("EventSource error", event)
			if (logEventSource) logEventSource.close()
		}
	}
	const { root, addLogEntries, onError } = logsSearchPage({
		isStreaming,
		toggleIsStreaming: () => {
			isStreaming = !isStreaming
			if (isStreaming) {
				startStream(query)
				setQueryParam("stream", "true")
			} else {
				if (logEventSource) logEventSource.close()
				removeQueryParam("stream")
			} 
			return isStreaming
		},
		fetchMore: (args) => {
			query = args.query
			if (query) setQueryParam("query", query)
			else removeQueryParam("query")
			console.log("fetchMore", args)
			const urlQuery = new URLSearchParams()
			const offsetInMinutes = new Date().getTimezoneOffset();
			const offsetInHours = -offsetInMinutes / 60;
			urlQuery.append("timezone", offsetInHours.toString())
			if (args.query) urlQuery.append("query", args.query)
			urlQuery.append("count", args.count.toString())
			urlQuery.append("offset", args.offset.toString())
			const url = new URL("/api/logs", window.location.origin)
			url.search = urlQuery.toString()
			fetch(url.toString()).then(async (res) => {
				if (res.status === 400) {
					const err = await res.json()
					console.error("res.error", err)
					onError(err.error)
					console.log("res", res)
					throw new Error("Failed to fetch logs")
				}
				return res.json()
			}).then((data) => {
				addLogEntries(data)
			}).catch((err) => {
				console.error("error", err)
			})
			if (isStreaming) startStream(query)
			else {
				if (logEventSource) {
					logEventSource.close()
				}
			}
		}
	})
	return root
}