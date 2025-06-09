import { showModal } from "./common";

export const PivotPage = (root: HTMLElement) => {
	// 1) Fake log data
        const fakeData: Record<string, unknown>[] = [
                { logLevel: 'Info', deviceId: 'Device1', message: 'Started process', timestamp: 1610000000000 },
                { logLevel: 'Error', deviceId: 'Device2', message: 'Failed to load module', timestamp: 1610000001000 },
                { logLevel: 'Warning', deviceId: 'Device1', message: 'Memory usage high', timestamp: 1610000002000 },
                { logLevel: 'Info', deviceId: 'Device3', message: 'Process completed', timestamp: 1610000003000 },
                { logLevel: 'Error', deviceId: 'Device1', message: 'Unhandled exception', timestamp: 1610000004000 },
                { logLevel: 'Debug', deviceId: 'Device2', message: 'Debugging info', timestamp: 1610000005000 },
        ];

	// 2) Available fields for grouping
	const availableFields = ['logLevel', 'deviceId', 'timestamp', 'message'];

	// 3) Main container
	const container = document.createElement('div');
	container.style.display = 'flex';
	container.style.flexDirection = 'column';
	container.style.gap = '16px';
	container.style.fontFamily = 'Arial, sans-serif';

	// ---------------------------------------------------------------------
	// A) "Configure Fields" Button
	// ---------------------------------------------------------------------
	const configureButton = document.createElement('button');
	configureButton.textContent = 'Configure Fields';
	configureButton.style.width = '150px';
	configureButton.style.padding = '8px';
	configureButton.style.cursor = 'pointer';
	container.appendChild(configureButton);

	// ---------------------------------------------------------------------
	// B) Modal for Field Selection
	// ---------------------------------------------------------------------
	const modalOverlay = document.createElement('div');
	modalOverlay.style.position = 'fixed';
	modalOverlay.style.top = '0';
	modalOverlay.style.left = '0';
	modalOverlay.style.width = '100%';
	modalOverlay.style.height = '100%';
	modalOverlay.style.backgroundColor = 'rgba(0, 0, 0, 0.5)';
	modalOverlay.style.display = 'none'; // Hidden by default
	modalOverlay.style.justifyContent = 'center';
	modalOverlay.style.alignItems = 'center';
	modalOverlay.style.zIndex = '9999';

	const modalContent = document.createElement('div');
	modalContent.style.background = '#fff';
	modalContent.style.padding = '16px';
	modalContent.style.borderRadius = '4px';
	modalContent.style.minWidth = '200px';

	const modalTitle = document.createElement('h3');
	modalTitle.textContent = 'Drag a Field';
	modalContent.appendChild(modalTitle);

	// Close button inside the modal
	const closeModalBtn = document.createElement('button');
	closeModalBtn.textContent = 'Close';
	closeModalBtn.style.marginBottom = '8px';
	closeModalBtn.addEventListener('click', () => {
		modalOverlay.style.display = 'none';
	});
	modalContent.appendChild(closeModalBtn);

	// List of draggable fields
	availableFields.forEach((field) => {
		const fieldDiv = document.createElement('div');
		fieldDiv.textContent = field;
		fieldDiv.draggable = true;
		fieldDiv.style.border = '1px solid #ccc';
		fieldDiv.style.padding = '4px 8px';
		fieldDiv.style.margin = '4px 0';
		fieldDiv.style.cursor = 'move';
		fieldDiv.style.backgroundColor = '#f9f9f9';

		fieldDiv.addEventListener('dragstart', (event) => {
			event.dataTransfer?.setData('text/plain', field);
			event.dataTransfer!.effectAllowed = 'move';
			// Optional: visually indicate dragging
			fieldDiv.style.opacity = '0.5';
		});

		fieldDiv.addEventListener('dragend', () => {
			fieldDiv.style.opacity = '1';
		});

		modalContent.appendChild(fieldDiv);
	});

	modalOverlay.appendChild(modalContent);
	document.body.appendChild(modalOverlay);

	// Open the modal on button click
        configureButton.addEventListener('click', () => {
                const hello = document.createElement('h1');
                hello.textContent = 'Hello';
                showModal({ title: 'Info', content: hello, footer: [] });
        });

	// ---------------------------------------------------------------------
	// C) Drop Zone for selecting a grouping field
	// ---------------------------------------------------------------------
	const dropZone = document.createElement('div');
	dropZone.innerHTML = '<h3>Drop a field here to group by</h3>';
	dropZone.style.border = '2px dashed #ccc';
	dropZone.style.padding = '16px';
	dropZone.style.margin = '16px 0';
	dropZone.style.textAlign = 'center';
	dropZone.style.minHeight = '50px';
	container.appendChild(dropZone);

	// Keep track of current grouping field
	let currentGroupField = 'logLevel';

	// ---------------------------------------------------------------------
	// D) Render Pivot Table
	// ---------------------------------------------------------------------
        const renderPivotTable = (groupField: string) => {
		// Remove existing pivot table if it exists
		const existingTable = container.querySelector('table');
		if (existingTable) {
			container.removeChild(existingTable);
		}

		// Calculate pivot data: group by the selected field and count occurrences
                const pivotResult: Record<string, number> = fakeData.reduce((acc: Record<string, number>, entry) => {
                        const key = String(entry[groupField]);
                        acc[key] = (acc[key] || 0) + 1;
                        return acc;
                }, {} as Record<string, number>);

		// Create table element
		const table = document.createElement('table');
		table.style.borderCollapse = 'collapse';
		table.style.width = '100%';

		// Table header
		const thead = document.createElement('thead');
		const headerRow = document.createElement('tr');

		const colHeader = document.createElement('th');
		colHeader.textContent = groupField;
		colHeader.style.border = '1px solid #ccc';
		colHeader.style.padding = '8px';
		colHeader.style.background = '#f9f9f9';
		headerRow.appendChild(colHeader);

		const countHeader = document.createElement('th');
		countHeader.textContent = 'Count';
		countHeader.style.border = '1px solid #ccc';
		countHeader.style.padding = '8px';
		countHeader.style.background = '#f9f9f9';
		headerRow.appendChild(countHeader);

		thead.appendChild(headerRow);
		table.appendChild(thead);

		// Table body
		const tbody = document.createElement('tbody');
                (Object.entries(pivotResult) as [string, number][]).forEach(([key, count]) => {
			const row = document.createElement('tr');

			const keyTd = document.createElement('td');
			keyTd.textContent = key;
			keyTd.style.border = '1px solid #ccc';
			keyTd.style.padding = '8px';
			row.appendChild(keyTd);

			const countTd = document.createElement('td');
			countTd.textContent = count.toString();
			countTd.style.border = '1px solid #ccc';
			countTd.style.padding = '8px';
			row.appendChild(countTd);

			tbody.appendChild(row);
		});
		table.appendChild(tbody);

		// Append the new table to the container
		container.appendChild(table);
	};

	// ---------------------------------------------------------------------
	// E) Drop Zone Event Handlers
	// ---------------------------------------------------------------------
	dropZone.addEventListener('dragover', (event) => {
		event.preventDefault(); // Necessary to allow drop
		dropZone.style.backgroundColor = '#eef';
	});

	dropZone.addEventListener('dragleave', () => {
		dropZone.style.backgroundColor = '';
	});

	dropZone.addEventListener('drop', (event) => {
		event.preventDefault();
		dropZone.style.backgroundColor = '';
		const field = event.dataTransfer?.getData('text/plain');
		if (field && availableFields.includes(field)) {
			currentGroupField = field;
			dropZone.innerHTML = `<h3>Grouping by: ${field}</h3>`;
			renderPivotTable(currentGroupField);
			// Hide the modal after a successful drop (optional)
			modalOverlay.style.display = 'none';
		}
	});

	// ---------------------------------------------------------------------
	// F) Initial Render
	// ---------------------------------------------------------------------
	// Show the default pivot table
	renderPivotTable(currentGroupField);

	// Finally, add everything to the root
	root.appendChild(container);
};