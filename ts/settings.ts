import { Container, UiComponent } from "./ui"

type Link = {
	href: string
	text: string
}

class LinkList extends UiComponent<HTMLDivElement> {
	constructor(links: Link[]) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexWrap = "wrap"
		this.root.style.gap = "10px"
		for (const link of links) {
			const item = document.createElement("div")
			item.className = "list-row"
			item.style.padding = "30px"
			this.root.appendChild(item)

			const linkElement = document.createElement("a")
			linkElement.href = link.href
			linkElement.innerText = link.text
			item.appendChild(linkElement)
		}
	}
}

export const settingsPage = (root: Container) => {
	root.root.innerHTML = ""
	const linkList = new LinkList([
		{ href: "/logs", text: "Logs" },
		{ href: "/devices", text: "Devices" },
		{ href: "/segments", text: "Segments" },
	])
	root.add(linkList)
}