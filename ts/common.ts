export const showModal = (content: HTMLElement, title: string) => {
	const body = document.querySelector("body")
  
	// Create and style the overlay
	const modalOverlay = document.createElement("div")
	modalOverlay.style.position = "fixed"
	modalOverlay.style.top = "0"
	modalOverlay.style.left = "0"
	modalOverlay.style.width = "100%"
	modalOverlay.style.height = "100%"
	modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)"
	modalOverlay.style.display = "flex" // Use flex to center content
	modalOverlay.style.justifyContent = "center"
	modalOverlay.style.alignItems = "center"
	modalOverlay.style.zIndex = "9999"
	body?.appendChild(modalOverlay)
  
	// Create and style the modal content
	const modalContent = document.createElement("div")
	modalContent.style.background = "#fff"
	modalContent.style.padding = "16px"
	modalContent.style.borderRadius = "4px"
	// Remove minWidth so the width is determined by its content
	modalContent.style.width = "auto"
  
	// Prevent clicks inside modal content from closing the modal
	modalContent.addEventListener("click", (e) => {
	  e.stopPropagation()
	})
  
	// Create and add title
	const modalTitle = document.createElement("h3")
	modalTitle.textContent = title
	modalContent.appendChild(modalTitle)
	
	// Add provided content
	modalContent.appendChild(content)
  
	// Create a close button and append it
	const closeModalBtn = document.createElement("button")
	closeModalBtn.textContent = "Close"
	closeModalBtn.style.marginTop = "8px"
	closeModalBtn.addEventListener("click", () => {
	  modalOverlay.remove()
	})
	modalContent.appendChild(closeModalBtn)
  
	// When clicking on the overlay (outside modalContent), remove the modal
	modalOverlay.addEventListener("click", () => {
	  modalOverlay.remove()
	})
  
	// Append modal content to overlay
	modalOverlay.appendChild(modalContent)
  }