export const formatLogMsg = (msg: string): HTMLElement => {
	const container = document.createElement("div")
	let jsonDepth = 0
	let backbuffer = ""
	for (const char of msg) {
		if (char === "{") {
			jsonDepth++
			if (jsonDepth === 1 && backbuffer) {
				const span = document.createElement("span")
				span.textContent = backbuffer
				container.appendChild(span)
				backbuffer = ""
			}
			backbuffer += char
			continue
		}
		if (char === "}") {
			jsonDepth--
			backbuffer += char
			if (jsonDepth === 0) {
				let trimmed = backbuffer.trim()
				if (trimmed.startsWith("{")) {
					const pre = document.createElement("pre")
					pre.textContent = JSON.stringify(JSON.parse(backbuffer), null, 2)
					container.appendChild(pre)
				} else if (trimmed.startsWith("<")) {
					try {
						const parser = new DOMParser()
						const xmlDoc = parser.parseFromString(backbuffer, "application/xml")
						if (!xmlDoc.getElementsByTagName("parsererror").length) {
							const pre = document.createElement("pre")
							pre.textContent = formatXml(backbuffer)
							container.appendChild(pre)
						} else {
							const span = document.createElement("span")
							span.textContent = backbuffer
							container.appendChild(span)
						}
					} catch (e) {
						const span = document.createElement("span")
						span.textContent = backbuffer
						container.appendChild(span)
					}
				} else {
					const span = document.createElement("span")
					span.textContent = backbuffer
					container.appendChild(span)
				}
				backbuffer = ""
				continue
			}
			continue
		}
		backbuffer += char
		if (jsonDepth === 0 && backbuffer.trim().startsWith("<") && char === ">") {
			try {
				const parser = new DOMParser()
				const xmlDoc = parser.parseFromString(backbuffer, "application/xml")
				if (!xmlDoc.getElementsByTagName("parsererror").length) {
					const pre = document.createElement("pre")
					pre.textContent = formatXml(backbuffer)
					container.appendChild(pre)
					backbuffer = ""
					continue
				}
			} catch (e) { }
		}
	}
	if (backbuffer) {
		let trimmed = backbuffer.trim()
		if (trimmed.startsWith("{")) {
			const pre = document.createElement("pre")
			pre.textContent = JSON.stringify(JSON.parse(backbuffer), null, 2)
			container.appendChild(pre)
		} else if (trimmed.startsWith("<")) {
			try {
				const parser = new DOMParser()
				const xmlDoc = parser.parseFromString(backbuffer, "application/xml")
				if (!xmlDoc.getElementsByTagName("parsererror").length) {
					const pre = document.createElement("pre")
					pre.textContent = formatXml(backbuffer)
					container.appendChild(pre)
				} else {
					const span = document.createElement("span")
					span.textContent = backbuffer
					container.appendChild(span)
				}
			} catch (e) {
				const span = document.createElement("span")
				span.textContent = backbuffer
				container.appendChild(span)
			}
		} else {
			const span = document.createElement("span")
			span.textContent = backbuffer
			container.appendChild(span)
		}
	}
	return container
}

const formatXml = (xml: string): string => {
	let formatted = ""
	xml = xml.replace(/(>)(<)(\/*)/g, "$1\n$2$3")
	let pad = 0
	xml.split("\n").forEach(node => {
		let indent = 0
		if (node.match(/.+<\/\w[^>]*>$/)) {
			indent = 0
		} else if (node.match(/^<\/\w/)) {
			if (pad !== 0) pad--
		} else if (node.match(/^<\w([^>]*[^\/])?>.*$/)) {
			indent = 1
		} else {
			indent = 0
		}
		formatted += "  ".repeat(pad) + node + "\n"
		pad += indent
	})
	return formatted
}
