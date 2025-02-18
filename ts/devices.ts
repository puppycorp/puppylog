export const devicesPage = async (root: HTMLElement) => {
	root.innerHTML = `
		<div class="page-header">
			<h1>Devices</h1>
		</div>
		<div class="logs-list" id="devicesList">
		<div class="logs-loading-indicator">Loading devices...</div>
		</div>
	`;
	try {
		const res = await fetch("/api/v1/devices").then((res) => res.json());
		const devicesList = document.getElementById("devicesList");
		if (!devicesList) return;
		devicesList.innerHTML = "";
		if (Array.isArray(res) && res.length > 0) {
			res.forEach((device: any) => {
				const deviceRow = document.createElement("div");
				deviceRow.classList.add("list-row");
				deviceRow.innerHTML = `
					<div class="table-cell"><strong>ID:</strong> ${device.id}</div>
					<div class="table-cell"><strong>Created at:</strong> ${new Date(device.created_at).toLocaleString()}</div>
					<div class="table-cell"><strong>Filter level:</strong> ${device.filter_level}</div>
					<div class="table-cell"><strong>Last upload:</strong> ${new Date(device.last_upload_at).toLocaleString()}</div>
					<div class="table-cell"><strong>Logs count:</strong> ${device.logs_count}</div>
					<div class="table-cell"><strong>Logs size:</strong> ${device.logs_size} bytes</div>
					<div class="table-cell"><strong>Send logs:</strong> ${device.send_logs ? "Yes" : "No"}</div>
				`;

				devicesList.appendChild(deviceRow);
			});
		} else {
			devicesList.innerHTML = `<p>No devices found.</p>`;
		}
	} catch (error) {
		console.error("Error fetching devices:", error);
		const devicesList = document.getElementById("devicesList");
		if (devicesList) {
			devicesList.innerHTML = `<p>Error fetching devices. Please try again later.</p>`;
		}
	}
};