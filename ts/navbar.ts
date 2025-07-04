import { HList, UiComponent } from "./ui"

export class Navbar extends HList {
	public readonly logsLink: HTMLAnchorElement
	public readonly devicesLink: HTMLAnchorElement
	public readonly segmentsLink: HTMLAnchorElement
	private leftItems: HTMLElement[]

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
		// prepare left items
		this.leftItems = [this.logsLink, this.devicesLink, this.segmentsLink]
		// initial render of left and optional right items
		this.setRight(args?.right)
	}

	/**
	 * Sets or updates right-side components on the navbar.
	 * @param right optional array of HTMLElements or UiComponent<HTMLElement> to display on the right
	 */
	public setRight(right?: (HTMLElement | UiComponent<HTMLElement>)[]): void {
		// clear existing children
		while (this.root.firstChild) {
			this.root.removeChild(this.root.firstChild)
		}
		// re-add left items
		if (right && right.length) {
			const spacer = document.createElement("div")
			spacer.style.flex = "1"
			const rightEls = right.map((item) =>
				item instanceof HTMLElement ? item : item.root,
			)
			this.add(...this.leftItems, spacer, ...rightEls)
		} else {
			this.add(...this.leftItems)
		}
	}
}
