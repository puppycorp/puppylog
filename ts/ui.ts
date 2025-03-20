
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
	constructor() {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"
	}

	public add(...components: UiComponent<HTMLElement>[]) {
		this.root.append(...components.map(c => c.root))
	}
}

export class HList extends UiComponent<HTMLDivElement> {
	constructor() {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "row"
	}
	
}

type SelectOption = {
	value: string
	text: string
}

export class Select extends UiComponent<HTMLSelectElement> {
	constructor(args: {
		label?: string
		options: SelectOption[]
	}) {
		super(document.createElement("select"))
		args.options.forEach(option => {
			const optionEl = document.createElement("option")
			optionEl.value = option.value
			optionEl.textContent = option.text
			this.root.appendChild(optionEl)
		})
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

	constructor(args: { label: string; options: SelectOption[] }) {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "column"
		const labelEl = document.createElement("label")
		labelEl.textContent = args.label
		this.root.appendChild(labelEl)
		this.select = new Select({ options: args.options })
		this.root.appendChild(this.select.root)
	}

	public get value(): string {
		return this.select.value
	}

	public set onChange(callback: (value: string) => void) {
		this.select.onChange = callback
	}
}