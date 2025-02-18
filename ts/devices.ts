export const devicesPage = async (root: HTMLElement) => {
	// Clear the root element
	root.innerHTML = "<h1>Devices</h1>";

	// Fetch the devices data from the API
	try {
		const res = await fetch("/api/v1/devices").then((res) => res.json());
		console.log("res", res);

		// Check if we got an array of devices
		if (Array.isArray(res) && res.length > 0) {
			// Create a container for the list
			const listContainer = document.createElement("ul");
			listContainer.style.listStyle = "none";
			listContainer.style.padding = "0";

			res.forEach((device: any) => {
				const listItem = document.createElement("li");
				listItem.style.border = "1px solid #ccc";
				listItem.style.borderRadius = "4px";
				listItem.style.marginBottom = "10px";
				listItem.style.padding = "10px";

				// Build the inner HTML for each device
				listItem.innerHTML = `
			<div><strong>ID:</strong> ${device.id}</div>
			<div><strong>Created at:</strong> ${new Date(device.created_at).toLocaleString()}</div>
			<div><strong>Filter level:</strong> ${device.filter_level}</div>
			<div><strong>Last upload:</strong> ${new Date(device.last_upload_at).toLocaleString()}</div>
			<div><strong>Logs count:</strong> ${device.logs_count}</div>
			<div><strong>Logs size:</strong> ${device.logs_size} bytes</div>
			<div><strong>Send logs:</strong> ${device.send_logs ? "Yes" : "No"}</div>
		  `;

				listContainer.appendChild(listItem);
			});

			// Append the list to the root element
			root.appendChild(listContainer);
		} else {
			root.innerHTML += "<p>No devices found.</p>";
		}
	} catch (error) {
		console.error("Error fetching devices:", error);
		root.innerHTML += `<p>Error fetching devices. Please try again later.</p>`;
	}
};