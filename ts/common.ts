import { UiComponent } from "./ui"

export class Modal extends UiComponent<HTMLDivElement> {
	constructor() {
		super(document.createElement("div"))
		this.root.className = "modal"
	}
}

export const showModal = (args: {
	title: string
	minWidth?: number
	content: HTMLElement
	footer: UiComponent<HTMLElement>[]
}): (() => void) => {
	const body = document.querySelector("body")

	const modalOverlay = document.createElement("div")
	modalOverlay.style.position = "fixed"
	modalOverlay.style.top = "0"
	modalOverlay.style.left = "0"
	modalOverlay.style.width = "100%"
	modalOverlay.style.height = "100%"
	modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)"
	modalOverlay.style.display = "flex"
	modalOverlay.style.justifyContent = "center"
	modalOverlay.style.alignItems = "center"
	modalOverlay.style.zIndex = "9999"
	body?.appendChild(modalOverlay)

	const modalContent = document.createElement("div")
	modalContent.style.background = "#fff"
	modalContent.style.padding = "16px"
	modalContent.style.borderRadius = "4px"
	modalContent.style.width = "auto"
	modalContent.style.maxWidth = "calc(100vw - 32px)"
	modalContent.style.wordWrap = "break-word"
	modalContent.style.wordBreak = "break-all"
	if (args.minWidth) modalContent.style.minWidth = `${args.minWidth}px`

	modalContent.addEventListener("click", (e) => {
		e.stopPropagation()
	})

	const modalTitle = document.createElement("h3")
	modalTitle.textContent = args.title
	modalContent.appendChild(modalTitle)

	const modalBody = document.createElement("div")
	modalBody.style.overflowY = "auto"
	modalBody.style.maxHeight = "calc(90vh - 100px)"
	modalBody.appendChild(args.content)
	modalContent.appendChild(modalBody)

	// modalContent.append(...args.footer.map(f => f.root))

	const buttonContainer = document.createElement("div")
	buttonContainer.style.display = "flex"
	buttonContainer.style.justifyContent = "space-between"
	buttonContainer.style.marginTop = "8px"
	buttonContainer.append(...args.footer.map((f) => f.root))

	modalContent.appendChild(buttonContainer)

	modalOverlay.addEventListener("click", () => {
		modalOverlay.remove()
	})

	modalOverlay.appendChild(modalContent)

	return () => {
		modalOverlay.remove()
	}
}
