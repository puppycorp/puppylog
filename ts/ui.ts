export class UiComponent<T> {
	public readonly root: T

	constructor(root: T) {
		this.root = root
	}
}

export class Container extends UiComponent<HTMLElement> {
	constructor(root: HTMLElement) {
		super(root)
	}

	public add(...components: UiComponent<HTMLElement>[]) {
		this.root.append(...components.map(c => c.root))
	}
}

export class VList extends UiComponent<HTMLDivElement> {
	constructor(args?: {
		style?: Partial<CSSStyleDeclaration>
	}) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"
		if (args?.style) Object.assign(this.root.style, args.style)
	}

	public add(...components: UiComponent<HTMLElement>[]) {
		this.root.append(...components.map(c => c.root))
		return this
	}
}

// export const vlist = (...components: (UiComponent<HTMLElement> | HTMLElement)[]) => {
// 	const root = document.createElement("div")
// 	root.style.display = "flex"
// 	root.style.flexDirection = "column"
// 	root.append(...components.map(c => c instanceof HTMLElement ? c : c.root))
// 	return root
// }

export class HList extends UiComponent<HTMLDivElement> {
	constructor() {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "row"
	}

	public add(...components: (UiComponent<HTMLElement> | HTMLElement)[]) {
		this.root.append(...components.map(c => c instanceof HTMLElement ? c : c.root))
	}
}

export class Button extends UiComponent<HTMLButtonElement> {
	constructor(args: { text: string }) {
		super(document.createElement("button"))
		this.root.textContent = args.text
	}

	public set onClick(callback: () => void) {
		this.root.onclick = callback
	}
}

export class Label extends UiComponent<HTMLLabelElement> {
	constructor(args: { text: string }) {
		super(document.createElement("label"))
		this.root.textContent = args.text
	}
}

type SelectOption = {
	value: string
	text: string
}

export class Select extends UiComponent<HTMLSelectElement> {
	constructor(args: {
		label?: string
		value?: string
		options: SelectOption[]
	}) {
		super(document.createElement("select"))
		args.options.forEach(option => {
			const optionEl = document.createElement("option")
			optionEl.value = option.value
			optionEl.textContent = option.text
			this.root.appendChild(optionEl)
		})
		this.root.value = args.value || ""
	}

	public get value(): string {
		return this.root.value
	}

	public set onChange(callback: (value: string) => void) {
		this.root.onchange = () => callback(this.root.value)
	}
}

export class SelectGroup extends UiComponent<HTMLDivElement> {
	private select: Select

	constructor(args: {
		label: string
		value: string
		options: SelectOption[]
	}) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"
		const labelEl = document.createElement("label")
		labelEl.textContent = args.label
		this.root.appendChild(labelEl)
		this.select = new Select({
			value: args.value,
			options: args.options
		})
		this.root.appendChild(this.select.root)
	}

	public get value(): string {
		return this.select.value
	}

	public set onChange(callback: (value: string) => void) {
		this.select.onChange = callback
	}
}

export class TextInput extends UiComponent<HTMLDivElement> {
	private input: HTMLInputElement

	constructor(args: {
		label?: string
		value?: string
		placeholder?: string
	}) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"

		if (args.label) {
			const labelEl = document.createElement("label")
			labelEl.textContent = args.label
			this.root.appendChild(labelEl)
		}

		this.input = document.createElement("input")
		this.input.type = "text"
		this.input.placeholder = args.placeholder || ""
		this.input.value = args.value || ""
		this.root.appendChild(this.input)
	}

	public get value(): string {
		return this.input.value
	}
}

export class MultiCheckboxSelect extends UiComponent<HTMLDivElement> {
	private checkboxes: HTMLInputElement[] = []
	private checkboxContainer: HTMLDivElement

	constructor(args: {
		label?: string
		options: { value: string, text: string, checked?: boolean }[]
		expanded?: boolean
	}) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"

		// Determine initial expanded state (default to false if not provided)
		const isExpanded = args.expanded !== undefined ? args.expanded : false

		// Create a header element that will act as a toggle
		const header = document.createElement("div")
		header.style.display = "flex"
		header.style.alignItems = "center"
		header.style.cursor = "pointer"

		// Create a toggle icon
		const toggleIcon = document.createElement("span")
		toggleIcon.textContent = isExpanded ? "▾" : "▸"
		toggleIcon.style.marginRight = "5px"

		// If a label is provided, add it to the header
		if (args.label) {
			const labelEl = document.createElement("label")
			labelEl.textContent = args.label
			header.appendChild(toggleIcon)
			header.appendChild(labelEl)
		} else {
			header.appendChild(toggleIcon)
		}

		this.root.appendChild(header)

		// Create a container for the checkboxes
		this.checkboxContainer = document.createElement("div")
		this.checkboxContainer.style.display = isExpanded ? "flex" : "none"
		this.checkboxContainer.style.flexDirection = "column"
		this.root.appendChild(this.checkboxContainer)

		// Add each option as a checkbox inside the container
		for (const option of args.options) {
			const container = document.createElement("div")
			container.style.display = "flex"
			container.style.alignItems = "center"

			const checkbox = document.createElement("input")
			checkbox.type = "checkbox"
			checkbox.value = option.value
			checkbox.checked = option.checked || false

			const optionLabel = document.createElement("span")
			optionLabel.textContent = option.text
			optionLabel.style.marginLeft = "5px"

			container.appendChild(checkbox)
			container.appendChild(optionLabel)
			this.checkboxContainer.appendChild(container)

			this.checkboxes.push(checkbox)
		}

		// Setup toggle functionality: collapse/expand the checkbox container
		header.onclick = () => {
			const isVisible = this.checkboxContainer.style.display !== "none"
			this.checkboxContainer.style.display = isVisible ? "none" : "flex"
			toggleIcon.textContent = isVisible ? "▸" : "▾"
		}
	}

	public get values(): string[] {
		return this.checkboxes.filter(chk => chk.checked).map(chk => chk.value)
	}

	public set onChange(callback: () => void) {
		this.checkboxes.forEach(checkbox => {
			checkbox.onchange = callback
		})
	}
}