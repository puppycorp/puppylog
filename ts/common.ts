export const showModal = (content: HTMLElement, title: string) => {
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
	modalContent.style.maxWidth = "500px"
	modalContent.style.wordWrap = "break-word"
	modalContent.style.wordBreak = "break-all"

	modalContent.addEventListener("click", (e) => {
		e.stopPropagation()
	})

	const modalTitle = document.createElement("h3")
	modalTitle.textContent = title
	modalContent.appendChild(modalTitle)

	modalContent.appendChild(content)

	const closeModalBtn = document.createElement("button")
	closeModalBtn.textContent = "Close"
	closeModalBtn.style.marginTop = "8px"
	closeModalBtn.addEventListener("click", () => {
		modalOverlay.remove()
	})
	modalContent.appendChild(closeModalBtn)

	modalOverlay.addEventListener("click", () => {
		modalOverlay.remove()
	})

	modalOverlay.appendChild(modalContent)
}
