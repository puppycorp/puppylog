import { LogRow, LogSearch, Logtable } from "./logs";

const generateFakeLogRows = (n: number): LogRow[] => {
	const rows: LogRow[] = []
	for (let i = 0; i < n; i++) {
		rows.push({
			timestamp: new Date().toISOString(),
			tags: ["APP", "INFO"],
			message: "hello world"
		})
	}
	return rows
}

window.onload = () => {
	const body = document.querySelector("body");

	if (!body) {
		throw new Error("No body element found")
	}
	
	const logSearch = new LogSearch()
	const t = new Logtable()
	body.appendChild(logSearch.root)
	body.appendChild(t.root)
}