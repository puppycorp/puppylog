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
	modalContent.style.maxWidth = "900px"
	modalContent.style.wordWrap = "break-word"
	modalContent.style.wordBreak = "break-all"

	modalContent.addEventListener("click", (e) => {
		e.stopPropagation()
	})

	const modalTitle = document.createElement("h3")
	modalTitle.textContent = title
	modalContent.appendChild(modalTitle)

	const modalBody = document.createElement("div")
	modalBody.style.overflowY = "auto"
	modalBody.style.maxHeight = "calc(90vh - 100px)"
	modalBody.appendChild(content)
	modalContent.appendChild(modalBody)

	const buttonContainer = document.createElement("div")
	buttonContainer.style.display = "flex"
	buttonContainer.style.justifyContent = "space-between"
	buttonContainer.style.marginTop = "8px"

	const copyBtn = document.createElement("button")
	copyBtn.textContent = "Copy"
	copyBtn.addEventListener("click", () => {
		navigator.clipboard.writeText(content.textContent || "").then(
			() => {
				console.log("Content copied to clipboard.")
			},
			(err) => {
				console.error("Failed to copy text: ", err)
			}
		)
	})
	buttonContainer.appendChild(copyBtn)

	const closeModalBtn = document.createElement("button")
	closeModalBtn.textContent = "Close"
	closeModalBtn.addEventListener("click", () => {
		modalOverlay.remove()
	})
	buttonContainer.appendChild(closeModalBtn)

	modalContent.appendChild(buttonContainer)

	modalOverlay.addEventListener("click", () => {
		modalOverlay.remove()
	})

	modalOverlay.appendChild(modalContent)
}