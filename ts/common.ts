
export const showModal = (content: HTMLElement) => {
	const body = document.querySelector("body");
	const modalOverlay = document.createElement("div");
	modalOverlay.style.position = "fixed";
	modalOverlay.style.top = "0";
	modalOverlay.style.left = "0";
	modalOverlay.style.width = "100%";
	modalOverlay.style.height = "100%";
	modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)";
	modalOverlay.style.display = "none"; // Hidden by default
	modalOverlay.style.justifyContent = "center";
	modalOverlay.style.alignItems = "center";
	modalOverlay.style.zIndex = "9999";
	body?.appendChild(modalOverlay)

	const modalContent = document.createElement("div");
	modalContent.style.background = "#fff";
	modalContent.style.padding = "16px";
	modalContent.style.borderRadius = "4px";
	modalContent.style.minWidth = "200px";

	const modalTitle = document.createElement("h3");
	modalTitle.textContent = "Drag a Field";
	modalContent.appendChild(modalTitle);

	// Close button inside the modal
	const closeModalBtn = document.createElement("button");
	closeModalBtn.textContent = "Close";
	closeModalBtn.style.marginBottom = "8px";
	closeModalBtn.addEventListener("click", () => {
		modalOverlay.style.display = "none";
	});
	modalContent.appendChild(closeModalBtn);
	body?.appendChild(modalContent);
}