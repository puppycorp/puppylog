import { HList, UiComponent } from "./ui"

export class Navbar extends HList {
	public readonly logsLink: HTMLAnchorElement
	public readonly devicesLink: HTMLAnchorElement
	public readonly segmentsLink: HTMLAnchorElement

	constructor(args?: { right?: (HTMLElement | UiComponent<HTMLElement>)[] }) {
		super()
		this.root.classList.add("page-header")
		this.root.style.gap = "8px"
		this.logsLink = document.createElement("a")
		this.logsLink.textContent = "Logs"
		this.logsLink.href = "/logs"
		this.logsLink.classList.add("link")
		this.devicesLink = document.createElement("a")
		this.devicesLink.textContent = "Devices"
		this.devicesLink.href = "/devices"
		this.devicesLink.classList.add("link")
		this.segmentsLink = document.createElement("a")
		this.segmentsLink.textContent = "Segments"
		this.segmentsLink.href = "/segments"
		this.segmentsLink.classList.add("link")
		// Mark active link
		const currentPath = window.location.pathname
		;[
			{ link: this.logsLink, path: "/logs" },
			{ link: this.devicesLink, path: "/devices" },
			{ link: this.segmentsLink, path: "/segments" },
		].forEach(({ link, path }) => {
			if (currentPath === path || currentPath.startsWith(path + "/")) {
				link.classList.add("active")
			}
		})
		// left items
		const leftItems = [this.logsLink, this.devicesLink, this.segmentsLink]
		if (args?.right && args.right.length) {
			const spacer = document.createElement("div")
			spacer.style.flex = "1"
			this.add(
				...leftItems,
				spacer,
				...args.right.map((item) =>
					item instanceof HTMLElement ? item : item.root,
				),
			)
		} else {
			this.add(...leftItems)
		}
	}
}
