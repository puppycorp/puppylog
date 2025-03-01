
export const showModal = (content: HTMLElement) => {
	const body = document.querySelector("body")
	console.log("show modal", body)
	const modalOverlay = document.createElement("div")
	modalOverlay.style.position = "absolute"
	modalOverlay.style.top = "0"
	modalOverlay.style.left = "0"
	modalOverlay.style.right = "0"
	modalOverlay.style.bottom = "0"
	// modalOverlay.style.width = "100%"
	// modalOverlay.style.height = "100%"
	modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)"
	//modalOverlay.style.display = "none" // Hidden by default
	modalOverlay.style.justifyContent = "center"
	modalOverlay.style.alignItems = "center"
	modalOverlay.style.zIndex = "9999"
	modalOverlay.onclick = (e) => {
		modalOverlay.remove()
	}
	body?.appendChild(modalOverlay)

	const modalContent = document.createElement("div")
	modalContent.style.background = "#fff"
	modalContent.style.padding = "16px"
	modalContent.style.borderRadius = "4px"
	modalContent.style.minWidth = "200px"

	// const modalTitle = document.createElement("h3")
	// modalTitle.textContent = "Drag a Field"
	// modalContent.appendChild(modalTitle)
	modalContent.appendChild(content)

	// Close button inside the modal
	const closeModalBtn = document.createElement("button")
	closeModalBtn.textContent = "Close"
	closeModalBtn.style.marginBottom = "8px"
	closeModalBtn.addEventListener("click", () => {
		modalOverlay.style.display = "none"
	})
	modalContent.appendChild(closeModalBtn)
	modalOverlay.appendChild(modalContent)
}

export const createQueryEditor = (query: string) => {
	const editor = document.createElement("div")
	editor.style.paddingLeft = "5px"
	editor.contentEditable = "true"
	return editor
}

const metricsSvg = `
<?xml version="1.0" encoding="iso-8859-1"?>
<!-- Uploaded to: SVG Repo, www.svgrepo.com, Generator: SVG Repo Mixer Tools -->
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg fill="#000000" height="800px" width="800px" version="1.1" id="Capa_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" 
	 viewBox="0 0 376 376" xml:space="preserve">
<g>
	<path d="M366,341H20V25c0-5.523-4.477-10-10-10S0,19.477,0,25v326c0,5.523,4.477,10,10,10h356c5.523,0,10-4.477,10-10
		C376,345.477,371.523,341,366,341z"/>
	<path d="M29.758,317.126h316.484c0.552,0,1-0.448,1-1c0-0.552-0.448-1-1-1H29.758c-0.552,0-1,0.448-1,1
		C28.758,316.678,29.206,317.126,29.758,317.126z"/>
	<path d="M28.758,256.126c0,0.552,0.448,1,1,1h36.187l55.228,26.46c0.694,0.333,1.431,0.492,2.158,0.492
		c1.656,0,3.264-0.825,4.211-2.301l15.821-24.651h202.879c0.552,0,1-0.448,1-1c0-0.552-0.448-1-1-1H144.646l32.876-51.224
		l43.374,39.217c1.852,1.674,4.655,1.727,6.568,0.121l54.789-45.988l27.58,17.389c2.335,1.472,5.423,0.772,6.896-1.563
		c1.473-2.336,0.773-5.424-1.563-6.896l-14.364-9.056h45.439c0.552,0,1-0.448,1-1c0-0.552-0.448-1-1-1h-48.611l-13.131-8.278
		c-1.839-1.158-4.216-0.998-5.881,0.4l-9.386,7.878h-86.506l-2.873-2.598c-1.09-0.985-2.555-1.44-4.014-1.247
		c-1.457,0.193-2.754,1.02-3.547,2.256l-1.02,1.589h-16.795l28.811-58h49.539l4.27,8.835c1.281,2.649,3.842,4.449,6.77,4.754
		c2.93,0.305,5.805-0.926,7.606-3.253l7.996-10.336h42.243l3.85,5.44c2.712,3.831,8.016,4.74,11.849,2.027
		c2.494-1.765,3.74-4.627,3.565-7.468h25.267c0.552,0,1-0.448,1-1c0-0.552-0.448-1-1-1h-25.629c-0.259-0.829-0.646-1.634-1.174-2.38
		l-30.666-43.334c-1.557-2.199-4.064-3.531-6.758-3.588c-2.7-0.078-5.255,1.166-6.904,3.297L246.428,126.2l-23.72-49.074h123.535
		c0.552,0,1-0.448,1-1c0-0.552-0.448-1-1-1H221.741l-5.588-11.561c-1.412-2.921-4.363-4.783-7.607-4.801
		c-3.271,0.013-6.215,1.813-7.658,4.719l-5.783,11.643H29.758c-0.552,0-1,0.448-1,1c0,0.552,0.448,1,1,1h164.353l-28.811,58h-36.751
		l-10.755-22.085c-1.42-2.916-4.376-4.77-7.619-4.779c-0.008,0-0.015,0-0.023,0c-3.234,0-6.189,1.836-7.622,4.738l-10.924,22.126
		H29.758c-0.552,0-1,0.448-1,1c0,0.552,0.448,1,1,1h60.861l-28.634,58H29.758c-0.552,0-1,0.448-1,1c0,0.552,0.448,1,1,1h31.24
		l-5.119,10.368c-2.078,4.209-0.351,9.307,3.859,11.385c1.209,0.598,2.493,0.881,3.756,0.881c3.133,0,6.147-1.74,7.628-4.74
		l8.834-17.894h60.155c1.586,1.931,3.957,3.107,6.522,3.118c0.011,0,0.022,0,0.033,0c2.575,0,4.96-1.178,6.554-3.118h16.768
		l-37.224,58H84.915L65.661,245.9c-2.491-1.193-5.477-0.141-6.67,2.35c-1.193,2.49-0.142,5.477,2.349,6.67l0.431,0.206H29.758
		C29.206,255.126,28.758,255.574,28.758,256.126z M281.53,108.607l18.767,26.519h-39.281L281.53,108.607z M208.396,86.581
		l23.464,48.545h-47.579L208.396,86.581z M164.307,137.126l-17.564,35.358l-17.219-35.358H164.307z M109.578,137.126h1.037
		l28.245,58H80.943L109.578,137.126z M266.85,197.126l-42.478,35.653l-39.434-35.653H266.85z M131.48,257.126l-9.969,15.534
		l-32.422-15.534H131.48z"/>
</g>
</svg>
`

const createMetricsButton = () => {
	const btn = document.createElement("button")
	btn.innerHTML = metricsSvg
	btn.style.width = "24px"
	btn.style.height = "24px"
	btn.style.padding = "0"
	btn.style.margin = "0"
	return btn
}

const histogramSvg = `
<?xml version="1.0" encoding="utf-8"?><!-- Uploaded to: SVG Repo, www.svgrepo.com, Generator: SVG Repo Mixer Tools -->
<svg width="800px" height="800px" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg"><path fill="#000000" d="M23 23v466h466v-18h-40.893V256h-48v215h-31.675V159.33h-48V471h-31.227V320.242h-48V471H207.2V80.418h-48V471H128V192H80v279H41V23H23z"/></svg>
`

const createHistogramButton = () => {
	const btn = document.createElement("button")
	btn.innerHTML = histogramSvg
	btn.style.width = "24px"
	btn.style.height = "24px"
	btn.style.padding = "0"
	btn.style.margin = "0"
	return btn
}

// const logl

interface Option {
	label: string
	onClick?: () => void
}

export const createMultiSwitch = (options: Option[]) => {
	const container = document.createElement("div")
	container.style.position = "relative"
	container.style.display = "flex"
	container.style.flexDirection = "row"
	container.style.justifyContent = "space-between"
	container.style.alignItems = "center"
	container.style.border = "1px solid #ddd"
	container.style.borderRadius = "4px"
	container.style.overflow = "hidden"
	container.style.fontFamily = "Arial, sans-serif"

	const slider = document.createElement("div")
	slider.style.position = "absolute"
	slider.style.top = "0"
	slider.style.left = "0"
	slider.style.height = "100%"
	slider.style.backgroundColor = "#007BFF"
	slider.style.transition = "left 0.3s ease, width 0.3s ease"
	container.appendChild(slider)

	const buttons: HTMLButtonElement[] = []

	options.forEach(option => {
		const btn = document.createElement("button")
		btn.textContent = option.label
		btn.style.flex = "1"
		btn.style.padding = "8px"
		btn.style.border = "none"
		btn.style.background = "transparent"
		btn.style.cursor = "pointer"
		btn.style.transition = "color 0.3s ease"
		btn.style.position = "relative"
		btn.style.zIndex = "1"
		btn.style.color = "#333"

		btn.addEventListener("mouseenter", () => {
			if (btn.style.color !== "white") {
				btn.style.color = "#555"
			}
		})
		btn.addEventListener("mouseleave", () => {
			if (btn.style.color !== "white") {
				btn.style.color = "#333"
			}
		})

		btn.addEventListener("click", () => {
			const btnRect = btn.getBoundingClientRect()
			const containerRect = container.getBoundingClientRect()
			const left = btnRect.left - containerRect.left
			slider.style.left = left + "px"
			slider.style.width = btnRect.width + "px"
			buttons.forEach(button => {
				button.style.color = "#333"
			})
			btn.style.color = "white"
			if (option.onClick) {
				option.onClick()
			}
		})

		container.appendChild(btn)
		buttons.push(btn)
	})

	if (buttons.length > 0) {
		requestAnimationFrame(() => {
			const firstBtn = buttons[0]
			const btnRect = firstBtn.getBoundingClientRect()
			const containerRect = container.getBoundingClientRect()
			slider.style.left = (btnRect.left - containerRect.left) + "px"
			slider.style.width = btnRect.width + "px"
			firstBtn.style.color = "white"
		})
	}

	return container
}
