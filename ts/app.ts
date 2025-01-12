import { LogEntry, LogSearchOptions, Logtable } from "./logs";
import { VirtualTable } from "./virtual-table";

const generateFakeLogRows = (n: number): LogEntry[] => {
	const rows: LogEntry[] = []
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
	
	const t = new Logtable()
	body.appendChild(t.root)

	// const tableElement = document.createElement("table")
	// const header = document.createElement("head")
	// header.innerHTML = `
	// <tr>
	// 	<th>Timestamp</th>
	// 	<th>Tags</th>
	// 	<th>Message</th>
	// </tr>
	// `
	// tableElement.appendChild(header)
	// const tableBody = document.createElement("tbody")
	// tableElement.appendChild(tableBody)

	// const table = new VirtualTable({
	// 	rowCount: 100000,
	// 	rowHeight: 20,
	// 	drawRow: (start, end) => {
	// 		console.log("Drawing rows", start, end)

	// 		let body = ""
	// 		for (let i = start; i < end; i++) {
	// 			body += `
	// 			<tr>
	// 				<td>${new Date().toISOString()}</td>
	// 				<td>APP, INFO</td>
	// 				<td>[${i}] line</td>
	// 			</tr>
	// 			`
	// 		}

	// 		tableBody.innerHTML = body
	// 		return tableElement
	// 	}
	// })
	// body.appendChild(table.root)
}