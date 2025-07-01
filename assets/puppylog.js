// ts/ui.ts
class UiComponent {
  root;
  constructor(root) {
    this.root = root;
  }
}

class Container extends UiComponent {
  constructor(root) {
    super(root);
  }
  add(...components) {
    this.root.append(...components.map((c) => c.root));
  }
}

class VList extends UiComponent {
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexDirection = "column";
    if (args?.style)
      Object.assign(this.root.style, args.style);
  }
  add(...components) {
    this.root.append(...components.map((c) => c.root));
    return this;
  }
}

class HList extends UiComponent {
  constructor() {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexDirection = "row";
  }
  add(...components) {
    this.root.append(...components.map((c) => c instanceof HTMLElement ? c : c.root));
  }
}

class Button extends UiComponent {
  constructor(args) {
    super(document.createElement("button"));
    this.root.textContent = args.text;
  }
  set onClick(callback) {
    this.root.onclick = callback;
  }
}

class Label extends UiComponent {
  constructor(args) {
    super(document.createElement("label"));
    this.root.textContent = args.text;
  }
}

class Select extends UiComponent {
  constructor(args) {
    super(document.createElement("select"));
    args.options.forEach((option) => {
      const optionEl = document.createElement("option");
      optionEl.value = option.value;
      optionEl.textContent = option.text;
      this.root.appendChild(optionEl);
    });
    this.root.value = args.value || "";
  }
  get value() {
    return this.root.value;
  }
  set onChange(callback) {
    this.root.onchange = () => callback(this.root.value);
  }
}

class SelectGroup extends UiComponent {
  select;
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexDirection = "column";
    const labelEl = document.createElement("label");
    labelEl.textContent = args.label;
    this.root.appendChild(labelEl);
    this.select = new Select({
      value: args.value,
      options: args.options
    });
    this.root.appendChild(this.select.root);
  }
  get value() {
    return this.select.value;
  }
  set onChange(callback) {
    this.select.onChange = callback;
  }
}

class TextInput extends UiComponent {
  input;
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexDirection = "column";
    if (args.label) {
      const labelEl = document.createElement("label");
      labelEl.textContent = args.label;
      this.root.appendChild(labelEl);
    }
    this.input = document.createElement("input");
    this.input.type = "text";
    this.input.placeholder = args.placeholder || "";
    this.input.value = args.value || "";
    this.root.appendChild(this.input);
  }
  get value() {
    return this.input.value;
  }
}

class MultiCheckboxSelect extends UiComponent {
  checkboxes = [];
  checkboxContainer;
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexDirection = "column";
    const isExpanded = args.expanded !== undefined ? args.expanded : false;
    const header = document.createElement("div");
    header.style.display = "flex";
    header.style.alignItems = "center";
    header.style.cursor = "pointer";
    const toggleIcon = document.createElement("span");
    toggleIcon.textContent = isExpanded ? "▾" : "▸";
    toggleIcon.style.marginRight = "5px";
    if (args.label) {
      const labelEl = document.createElement("label");
      labelEl.textContent = args.label;
      header.appendChild(toggleIcon);
      header.appendChild(labelEl);
    } else {
      header.appendChild(toggleIcon);
    }
    this.root.appendChild(header);
    this.checkboxContainer = document.createElement("div");
    this.checkboxContainer.style.display = isExpanded ? "flex" : "none";
    this.checkboxContainer.style.flexDirection = "column";
    this.checkboxContainer.style.maxHeight = "200px";
    this.checkboxContainer.style.overflowY = "auto";
    this.root.appendChild(this.checkboxContainer);
    for (const option of args.options) {
      const container = document.createElement("div");
      container.style.display = "flex";
      container.style.alignItems = "center";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.value = option.value;
      checkbox.checked = option.checked || false;
      const optionLabel = document.createElement("span");
      optionLabel.textContent = option.text;
      optionLabel.style.marginLeft = "5px";
      container.appendChild(checkbox);
      container.appendChild(optionLabel);
      this.checkboxContainer.appendChild(container);
      this.checkboxes.push(checkbox);
    }
    header.onclick = () => {
      const isVisible = this.checkboxContainer.style.display !== "none";
      this.checkboxContainer.style.display = isVisible ? "none" : "flex";
      toggleIcon.textContent = isVisible ? "▸" : "▾";
    };
  }
  get values() {
    return this.checkboxes.filter((chk) => chk.checked).map((chk) => chk.value);
  }
  set onChange(callback) {
    this.checkboxes.forEach((checkbox) => {
      checkbox.onchange = callback;
    });
  }
}

class InfiniteScroll extends UiComponent {
  isLoading;
  onLoadMoreCallback;
  sentinel;
  observer;
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.minHeight = "100px";
    this.root.appendChild(args.container.root);
    this.isLoading = false;
    this.onLoadMoreCallback = null;
    this.sentinel = document.createElement("div");
    this.sentinel.style.height = "1px";
    this.sentinel.style.marginTop = "1px";
    this.root.appendChild(this.sentinel);
    const observerRoot = args.container.root;
    const options = {
      threshold: 0.1
    };
    this.observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (!this.isLoading && entry.isIntersecting) {
          this.loadMore();
        }
      });
    }, options);
    this.observer.observe(this.sentinel);
  }
  async loadMore() {
    this.isLoading = true;
    if (this.onLoadMoreCallback) {
      await this.onLoadMoreCallback();
    }
    this.isLoading = false;
  }
  set onLoadMore(callback) {
    this.onLoadMoreCallback = callback;
  }
}

class Header extends UiComponent {
  constructor(args) {
    super(document.createElement("div"));
    this.root.className = "page-header";
    const title = document.createElement("h1");
    title.textContent = args.title;
    title.style.flexGrow = "1";
    this.root.appendChild(title);
    if (args.rightSide) {
      this.root.append(args.rightSide.root);
    }
  }
}

class WrapList {
  root;
  constructor() {
    this.root = document.createElement("div");
    this.root.style.display = "flex";
    this.root.style.flexDirection = "row";
    this.root.style.flexWrap = "wrap";
    this.root.style.gap = "5px";
    this.root.style.overflowX = "auto";
    this.root.style.padding = "16px";
  }
  add(device) {
    this.root.appendChild(device.root);
  }
  set status(message) {
    this.root.innerHTML = `<p>${message}</p>`;
  }
  clear() {
    this.root.innerHTML = "";
  }
}

class KeyValueTable extends VList {
  constructor(items) {
    super();
    this.root.className = "list-row";
    for (const item of items) {
      const container = document.createElement("div");
      container.className = "table-cell";
      container.style.fontWeight = "bold";
      this.root.appendChild(container);
      const key = document.createElement("strong");
      key.textContent = item.key;
      container.appendChild(key);
      if (item.href) {
        const link = document.createElement("a");
        link.href = item.href;
        link.textContent = item.value;
        container.appendChild(link);
      } else {
        container.appendChild(document.createTextNode(`: ${item.value}`));
      }
    }
  }
}

class Collapsible extends UiComponent {
  expandButton;
  content;
  contentContainer;
  isOpen;
  constructor(args) {
    super(document.createElement("div"));
    this.root.style.position = "relative";
    this.expandButton = document.createElement("button");
    this.expandButton.textContent = args.buttonText;
    this.expandButton.style.cursor = "pointer";
    this.root.appendChild(this.expandButton);
    this.content = args.content;
    this.contentContainer = document.createElement("div");
    this.contentContainer.style.position = "absolute";
    this.contentContainer.style.top = "0";
    this.contentContainer.style.left = "100%";
    this.contentContainer.style.zIndex = "1000";
    this.contentContainer.style.display = "none";
    this.contentContainer.appendChild(this.content.root);
    this.root.appendChild(this.contentContainer);
    this.isOpen = false;
    this.expandButton.addEventListener("click", (e) => {
      e.stopPropagation();
      this.toggle();
    });
    document.addEventListener("click", this.handleDocumentClick.bind(this));
  }
  toggle() {
    if (this.isOpen) {
      this.hide();
    } else {
      this.show();
    }
  }
  show() {
    this.isOpen = true;
    this.contentContainer.style.display = "block";
    this.contentContainer.style.left = "100%";
    this.contentContainer.style.right = "auto";
    const rect = this.contentContainer.getBoundingClientRect();
    if (rect.right > window.innerWidth) {
      this.contentContainer.style.left = "auto";
      this.contentContainer.style.right = "100%";
    }
  }
  hide() {
    this.isOpen = false;
    this.contentContainer.style.display = "none";
  }
  handleDocumentClick(e) {
    if (!this.root.contains(e.target)) {
      this.hide();
    }
  }
}

// ts/common.ts
var showModal = (args) => {
  const body = document.querySelector("body");
  const modalOverlay = document.createElement("div");
  modalOverlay.style.position = "fixed";
  modalOverlay.style.top = "0";
  modalOverlay.style.left = "0";
  modalOverlay.style.width = "100%";
  modalOverlay.style.height = "100%";
  modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)";
  modalOverlay.style.display = "flex";
  modalOverlay.style.justifyContent = "center";
  modalOverlay.style.alignItems = "center";
  modalOverlay.style.zIndex = "9999";
  body?.appendChild(modalOverlay);
  const modalContent = document.createElement("div");
  modalContent.style.background = "#fff";
  modalContent.style.padding = "16px";
  modalContent.style.borderRadius = "4px";
  modalContent.style.width = "auto";
  modalContent.style.maxWidth = "calc(100vw - 32px)";
  modalContent.style.wordWrap = "break-word";
  modalContent.style.wordBreak = "break-all";
  if (args.minWidth)
    modalContent.style.minWidth = `${args.minWidth}px`;
  modalContent.addEventListener("click", (e) => {
    e.stopPropagation();
  });
  const modalTitle = document.createElement("h3");
  modalTitle.textContent = args.title;
  modalContent.appendChild(modalTitle);
  const modalBody = document.createElement("div");
  modalBody.style.overflowY = "auto";
  modalBody.style.maxHeight = "calc(90vh - 100px)";
  modalBody.appendChild(args.content);
  modalContent.appendChild(modalBody);
  const buttonContainer = document.createElement("div");
  buttonContainer.style.display = "flex";
  buttonContainer.style.justifyContent = "space-between";
  buttonContainer.style.marginTop = "8px";
  buttonContainer.append(...args.footer.map((f) => f.root));
  modalContent.appendChild(buttonContainer);
  modalOverlay.addEventListener("click", () => {
    modalOverlay.remove();
  });
  modalOverlay.appendChild(modalContent);
};

// ts/utility.ts
var setQueryParam = (field, value) => {
  const url = new URL(window.location.href);
  url.searchParams.set(field, value);
  window.history.pushState({}, "", url.toString());
};
var getQueryParam = (field) => {
  const url = new URL(window.location.href);
  return url.searchParams.get(field);
};
var removeQueryParam = (field) => {
  const url = new URL(window.location.href);
  url.searchParams.delete(field);
  window.history.pushState({}, "", url.toString());
};
var formatBytes = (bytes, decimals = 2) => {
  if (bytes === 0)
    return "0 Bytes";
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i];
};
var formatNumber = (num, decimals = 2) => {
  if (num === 0)
    return "0";
  const k = 1000;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["", "K", "M", "B", "T"];
  const i = Math.floor(Math.log(Math.abs(num)) / Math.log(k));
  return parseFloat((num / Math.pow(k, i)).toFixed(dm)) + sizes[i];
};
var formatTimestamp = (timestamp) => {
  const d = new Date(timestamp);
  return d.toLocaleString();
};

// ts/devices.ts
var saveDeviceSettings = async (device) => {
  await fetch(`/api/v1/device/${device.id}/settings`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(device)
  });
};
var bulkEdit = async (args) => {
  await fetch(`/api/v1/device_bulkedit`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args)
  });
};
var levels = ["trace", "debug", "info", "warn", "error", "fatal"];

class DeviceRow extends UiComponent {
  device;
  constructor(device) {
    const deviceRow = document.createElement("div");
    deviceRow.classList.add("list-row");
    super(deviceRow);
    this.device = device;
    const idCell = document.createElement("div");
    idCell.className = "table-cell";
    idCell.innerHTML = `<strong>ID:</strong> ${device.id}`;
    this.root.appendChild(idCell);
    const createdAtCell = document.createElement("div");
    createdAtCell.className = "table-cell";
    createdAtCell.innerHTML = `<strong>Created at:</strong> ${new Date(device.createdAt).toLocaleString()}`;
    this.root.appendChild(createdAtCell);
    const filterLevelCell = document.createElement("div");
    filterLevelCell.className = "table-cell";
    filterLevelCell.innerHTML = `<strong>Filter level:</strong> `;
    const select = document.createElement("select");
    levels.forEach((level) => {
      const option = document.createElement("option");
      option.value = level;
      option.textContent = level;
      select.appendChild(option);
    });
    select.value = device.filterLevel;
    filterLevelCell.appendChild(select);
    this.root.appendChild(filterLevelCell);
    const sendIntervalCell = document.createElement("div");
    sendIntervalCell.className = "table-cell";
    sendIntervalCell.innerHTML = `<strong>Send interval:</strong> ${device.sendInterval} seconds`;
    this.root.appendChild(sendIntervalCell);
    const lastUploadCell = document.createElement("div");
    lastUploadCell.className = "table-cell";
    lastUploadCell.innerHTML = `<strong>Last upload:</strong> ${new Date(device.lastUploadAt).toLocaleString()}`;
    this.root.appendChild(lastUploadCell);
    const logsCountCell = document.createElement("div");
    logsCountCell.className = "table-cell";
    logsCountCell.innerHTML = `<strong>Logs count:</strong> ${formatNumber(device.logsCount)}`;
    this.root.appendChild(logsCountCell);
    const logsSizeCell = document.createElement("div");
    logsSizeCell.className = "table-cell";
    logsSizeCell.innerHTML = `<strong>Logs size:</strong> ${formatBytes(device.logsSize)} bytes`;
    this.root.appendChild(logsSizeCell);
    const averageLogSizeCell = document.createElement("div");
    averageLogSizeCell.className = "table-cell";
    averageLogSizeCell.innerHTML = `<strong>Average log size:</strong> ${formatBytes(device.logsSize / device.logsCount)}`;
    this.root.appendChild(averageLogSizeCell);
    const logsPerSecondCell = document.createElement("div");
    logsPerSecondCell.className = "table-cell";
    const lastUploadDate = new Date(device.lastUploadAt);
    const createdAtDate = new Date(device.createdAt);
    const diff = lastUploadDate.getTime() - createdAtDate.getTime();
    const seconds = diff / 1000;
    const logsPerSecond = device.logsCount / seconds;
    logsPerSecondCell.innerHTML = `<strong>Logs per second:</strong> ${logsPerSecond.toFixed(2)}`;
    this.root.appendChild(logsPerSecondCell);
    const sendLogsCell = document.createElement("div");
    sendLogsCell.className = "table-cell";
    sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`;
    this.root.appendChild(sendLogsCell);
    const propsContainer = document.createElement("div");
    propsContainer.className = "table-cell";
    const propsTitle = document.createElement("strong");
    propsTitle.textContent = "Props:";
    propsContainer.appendChild(propsTitle);
    if (device.props.length === 0) {
      const noPropsRow = document.createElement("div");
      noPropsRow.textContent = "No properties";
      propsContainer.appendChild(noPropsRow);
    } else {
      device.props.forEach((prop) => {
        const propRow = document.createElement("div");
        propRow.textContent = `${prop.key} = ${prop.value}`;
        propsContainer.appendChild(propRow);
      });
    }
    this.root.appendChild(propsContainer);
    const deviceSaveButton = document.createElement("button");
    deviceSaveButton.textContent = "Save";
    deviceSaveButton.style.visibility = "hidden";
    this.root.appendChild(deviceSaveButton);
    const markDirty = () => {
      deviceSaveButton.style.visibility = "visible";
    };
    select.onchange = () => {
      device.filterLevel = select.value;
      markDirty();
    };
    sendLogsCell.onclick = () => {
      device.sendLogs = !device.sendLogs;
      sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`;
      markDirty();
    };
    deviceSaveButton.onclick = async () => {
      await saveDeviceSettings(device);
      deviceSaveButton.style.visibility = "hidden";
    };
  }
}

class DevicesList {
  root;
  constructor() {
    this.root = document.createElement("div");
    this.root.style.display = "flex";
    this.root.style.flexDirection = "row";
    this.root.style.flexWrap = "wrap";
    this.root.style.gap = "5px";
    this.root.style.overflowX = "auto";
    this.root.style.padding = "16px";
    this.root.innerHTML = `<div class="logs-loading-indicator">Loading devices...</div>`;
  }
  add(device) {
    this.root.appendChild(device.root);
  }
  noDevicesFound() {
    this.root.innerHTML = `<p>No devices found.</p>`;
  }
  clear() {
    this.root.innerHTML = "";
  }
}

class Summary extends UiComponent {
  constructor() {
    super(document.createElement("div"));
    this.root.innerHTML = "";
  }
  setSummary(args) {
    this.root.innerHTML = `
			<div><strong>Total Devices Count:</strong> ${formatNumber(args.totalDevicesCount)}</div>
			<div><strong>Total Logs Count:</strong> ${formatNumber(args.totalLogsCount)}</div>
			<div><strong>Total Logs Size:</strong> ${formatBytes(args.totalLogsSize)}</div>
			<div><strong>Average Log Size:</strong> ${formatBytes(args.averageLogSize)}</div>
			<div><strong>Logs per Second:</strong> ${args.totalLogsPerSecond.toFixed(2)}</div>
		`;
  }
}
var devicesPage = async (root) => {
  const page = new Container(root);
  const summary = new Summary;
  const header = new Header({
    title: "Devices",
    rightSide: summary
  });
  const sendLogsSearchOption = new SelectGroup({
    label: "Sending logs",
    value: "all",
    options: [
      {
        text: "All",
        value: "all"
      },
      {
        text: "Yes",
        value: "true"
      },
      {
        text: "No",
        value: "false"
      }
    ]
  });
  const bulkEditButton = document.createElement("button");
  bulkEditButton.textContent = "Bulk Edit";
  const filterLevelMultiSelect = new MultiCheckboxSelect({
    label: "Filter level",
    options: levels.map((level) => ({ text: level, value: level }))
  });
  const propsFiltters = new HList;
  propsFiltters.root.style.gap = "10px";
  propsFiltters.root.style.flexWrap = "wrap";
  const searchOptions = new HList;
  searchOptions.root.style.flexWrap = "wrap";
  searchOptions.root.style.margin = "10px";
  searchOptions.root.style.gap = "10px";
  searchOptions.add(sendLogsSearchOption);
  searchOptions.add(filterLevelMultiSelect);
  searchOptions.add(propsFiltters);
  searchOptions.root.appendChild(bulkEditButton);
  const devicesList = new DevicesList;
  page.add(header, searchOptions, devicesList);
  try {
    const res = await fetch("/api/v1/devices");
    const devices = await res.json();
    let totalLogsCount = 0, totalLogsSize = 0;
    let earliestTimestamp = Infinity, latestTimestamp = -Infinity;
    let totalLogsPerSecond = 0;
    devices.forEach((device) => {
      totalLogsCount += device.logsCount;
      totalLogsSize += device.logsSize;
      const createdAtTime = new Date(device.createdAt).getTime();
      const lastUploadTime = new Date(device.lastUploadAt).getTime();
      earliestTimestamp = Math.min(earliestTimestamp, createdAtTime);
      latestTimestamp = Math.max(latestTimestamp, lastUploadTime);
      const logsPersecond = device.logsCount / ((lastUploadTime - createdAtTime) / 1000);
      if (!isNaN(logsPersecond))
        totalLogsPerSecond += logsPersecond;
    });
    const averageLogSize = totalLogsCount > 0 ? totalLogsSize / totalLogsCount : 0;
    summary.setSummary({
      totalDevicesCount: devices.length,
      totalLogsCount,
      totalLogsSize,
      averageLogSize,
      totalLogsPerSecond
    });
    const renderList = (devices2) => {
      devicesList.clear();
      if (Array.isArray(devices2) && devices2.length > 0) {
        for (const device of devices2) {
          devicesList.add(new DeviceRow(device));
        }
      } else {
        devicesList.noDevicesFound();
      }
    };
    renderList(devices);
    let filteredDevices = devices;
    let filterLevel = [];
    let sendLogsFilter = undefined;
    let filtterProps = new Map;
    const filterDevices = () => {
      filteredDevices = devices.filter((device) => {
        if (sendLogsFilter !== undefined && device.sendLogs !== sendLogsFilter)
          return false;
        if (filterLevel.length > 0 && !filterLevel.includes(device.filterLevel))
          return false;
        for (const [key, values] of filtterProps) {
          if (!device.props.some((prop) => prop.key === key && values.includes(prop.value)))
            return false;
        }
        return true;
      });
      renderList(filteredDevices);
    };
    const uniquePropKeys = Array.from(new Set(devices.flatMap((device) => device.props.map((prop) => prop.key))));
    for (const key of uniquePropKeys) {
      const uniqueValues = Array.from(new Set(devices.flatMap((device) => device.props.filter((prop) => prop.key === key).map((prop) => prop.value))));
      const options = uniqueValues.map((value) => ({
        text: value,
        value
      }));
      const multiSelect = new MultiCheckboxSelect({
        label: key,
        options
      });
      multiSelect.onChange = () => {
        if (multiSelect.values.length === 0)
          filtterProps.delete(key);
        else
          filtterProps.set(key, multiSelect.values);
        filterDevices();
      };
      propsFiltters.add(multiSelect);
    }
    filterLevelMultiSelect.onChange = () => {
      filterLevel = filterLevelMultiSelect.values;
      filterDevices();
    };
    sendLogsSearchOption.onChange = async (value) => {
      sendLogsFilter = value === "all" ? undefined : value === "true";
      filterDevices();
    };
    bulkEditButton.onclick = () => {
      const first = filteredDevices[0];
      if (!first)
        return;
      const bulkEditFilterLevel = new SelectGroup({
        label: "Filter level",
        value: first.filterLevel,
        options: levels.map((level) => ({ text: level, value: level }))
      });
      const sendLogsSelect = new SelectGroup({
        label: "Send logs",
        value: first.sendLogs ? "true" : "false",
        options: [
          { text: "Yes", value: "true" },
          { text: "No", value: "false" }
        ]
      });
      const sendIntervalInput = new TextInput({
        label: "Send interval",
        placeholder: "Enter interval",
        value: first.sendInterval.toString()
      });
      const saveButton = new Button({ text: "Save" });
      saveButton.onClick = async () => {
        const filterLevel2 = bulkEditFilterLevel.value;
        const sendLogs = sendLogsSelect.value === "true";
        await bulkEdit({
          deviceIds: filteredDevices.map((p) => p.id),
          filterLevel: filterLevel2,
          sendInterval: parseInt(sendIntervalInput.value),
          sendLogs
        });
        for (const device of filteredDevices) {
          device.filterLevel = filterLevel2;
          device.sendLogs = sendLogs;
          device.sendInterval = parseInt(sendIntervalInput.value);
        }
        renderList(filteredDevices);
      };
      showModal({
        title: "Bulk Edit",
        minWidth: 300,
        content: new VList({
          style: {
            gap: "10px"
          }
        }).add(bulkEditFilterLevel, sendLogsSelect, sendIntervalInput, new Label({ text: "Devices: " }), new Label({
          text: filteredDevices.map((p) => p.id).join(", ")
        })).root,
        footer: [saveButton]
      });
    };
  } catch (error) {
    console.error("Error fetching devices:", error);
    const devicesList2 = document.getElementById("devicesList");
    if (devicesList2) {
      devicesList2.innerHTML = `<p>Error fetching devices. Please try again later.</p>`;
    }
  }
};

// ts/logmsg.ts
var formatLogMsg = (msg) => {
  const container = document.createElement("div");
  let jsonDepth = 0;
  let backbuffer = "";
  for (const char of msg) {
    if (char === "{") {
      jsonDepth++;
      if (jsonDepth === 1 && backbuffer) {
        const span = document.createElement("span");
        span.textContent = backbuffer;
        container.appendChild(span);
        backbuffer = "";
      }
      backbuffer += char;
      continue;
    }
    if (char === "}") {
      jsonDepth--;
      backbuffer += char;
      if (jsonDepth === 0) {
        let trimmed = backbuffer.trim();
        if (trimmed.startsWith("{")) {
          try {
            const pre = document.createElement("pre");
            pre.textContent = JSON.stringify(JSON.parse(backbuffer), null, 2);
            container.appendChild(pre);
          } catch (e) {
            const span = document.createElement("span");
            span.textContent = backbuffer;
            container.appendChild(span);
          }
        } else if (trimmed.startsWith("<")) {
          try {
            const parser = new DOMParser;
            const xmlDoc = parser.parseFromString(backbuffer, "application/xml");
            if (!xmlDoc.getElementsByTagName("parsererror").length) {
              const pre = document.createElement("pre");
              pre.textContent = formatXml(backbuffer);
              container.appendChild(pre);
            } else {
              const span = document.createElement("span");
              span.textContent = backbuffer;
              container.appendChild(span);
            }
          } catch (e) {
            const span = document.createElement("span");
            span.textContent = backbuffer;
            container.appendChild(span);
          }
        } else {
          const span = document.createElement("span");
          span.textContent = backbuffer;
          container.appendChild(span);
        }
        backbuffer = "";
        continue;
      }
      continue;
    }
    backbuffer += char;
    if (jsonDepth === 0 && backbuffer.trim().startsWith("<") && char == ">") {
      try {
        const parser = new DOMParser;
        const xmlDoc = parser.parseFromString(backbuffer, "application/xml");
        if (!xmlDoc.getElementsByTagName("parsererror").length) {
          const pre = document.createElement("pre");
          pre.textContent = formatXml(backbuffer);
          container.appendChild(pre);
          backbuffer = "";
          continue;
        }
      } catch (e) {}
    }
  }
  if (backbuffer) {
    let trimmed = backbuffer.trim();
    if (trimmed.startsWith("{")) {
      try {
        const pre = document.createElement("pre");
        pre.textContent = JSON.stringify(JSON.parse(backbuffer), null, 2);
        container.appendChild(pre);
      } catch (e) {
        const span = document.createElement("span");
        span.textContent = backbuffer;
        container.appendChild(span);
      }
    } else if (trimmed.startsWith("<")) {
      try {
        const parser = new DOMParser;
        const xmlDoc = parser.parseFromString(backbuffer, "application/xml");
        if (!xmlDoc.getElementsByTagName("parsererror").length) {
          const pre = document.createElement("pre");
          pre.textContent = formatXml(backbuffer);
          container.appendChild(pre);
        } else {
          const span = document.createElement("span");
          span.textContent = backbuffer;
          container.appendChild(span);
        }
      } catch (e) {
        const span = document.createElement("span");
        span.textContent = backbuffer;
        container.appendChild(span);
      }
    } else {
      const span = document.createElement("span");
      span.textContent = backbuffer;
      container.appendChild(span);
    }
  }
  return container;
};
var formatXml = (xml) => {
  let formatted = "";
  xml = xml.replace(/(>)(<)(\/*)/g, `$1
$2$3`);
  let pad = 0;
  xml.split(`
`).forEach((node) => {
    let indent = 0;
    if (node.match(/.+<\/\w[^>]*>$/)) {
      indent = 0;
    } else if (node.match(/^<\/\w/)) {
      if (pad !== 0)
        pad--;
    } else if (node.match(/^<\w([^>]*[^\/])?>.*$/)) {
      indent = 1;
    } else {
      indent = 0;
    }
    formatted += "  ".repeat(pad) + node + `
`;
    pad += indent;
  });
  return formatted;
};

// ts/pattern-matcher.ts
function patternMatcher(handlers) {
  const typedHandlers = handlers;
  const routes = Object.keys(typedHandlers).sort((a, b) => {
    if (!a.includes("*") && !a.includes(":"))
      return -1;
    if (!b.includes("*") && !b.includes(":"))
      return 1;
    if (a.includes(":") && !b.includes(":"))
      return -1;
    if (!a.includes(":") && b.includes(":"))
      return 1;
    if (a.includes("*") && !b.includes("*"))
      return 1;
    if (!a.includes("*") && b.includes("*"))
      return -1;
    return b.length - a.length;
  });
  return {
    match(path) {
      for (const route of routes) {
        const params = matchRoute(route, path);
        if (params !== null) {
          const result = typedHandlers[route](params);
          return { pattern: route, result };
        }
      }
      return null;
    }
  };
}
function matchRoute(pattern, path) {
  const patternParts = pattern.split("/").filter((segment) => segment.length > 0);
  const pathParts = path.split("/").filter((segment) => segment.length > 0);
  if (pattern === "/*")
    return {};
  if (patternParts.length !== pathParts.length) {
    const lastPattern = patternParts[patternParts.length - 1];
    if (lastPattern === "*" && pathParts.length >= patternParts.length - 1) {
      return {};
    }
    return null;
  }
  const params = {};
  for (let i = 0;i < patternParts.length; i++) {
    const patternPart = patternParts[i];
    const pathPart = pathParts[i];
    if (patternPart === "*") {
      return params;
    }
    if (patternPart.startsWith(":")) {
      const paramName = patternPart.slice(1);
      params[paramName] = pathPart;
    } else if (patternPart !== pathPart) {
      return null;
    }
  }
  return params;
}

// ts/router.ts
var matcher;
var handleRoute = (path) => {
  if (!matcher)
    return;
  const m = matcher.match(path);
  if (!m)
    console.error("No route found for", path);
  console.log("match result", m);
};
window.addEventListener("popstate", () => {
  handleRoute(window.location.pathname);
});
var routes = (routes2) => {
  matcher = patternMatcher(routes2);
  handleRoute(window.location.pathname);
};
var navigate = (path) => {
  window.history.pushState({}, "", path);
  handleRoute(path);
};

// ts/queries.ts
var loadSavedQueries = () => {
  try {
    const raw = localStorage.getItem("savedQueries");
    if (!raw)
      return [];
    return JSON.parse(raw);
  } catch {
    return [];
  }
};
var saveQuery = (item) => {
  const items = loadSavedQueries();
  items.push(item);
  localStorage.setItem("savedQueries", JSON.stringify(items));
};
var queriesPage = (root) => {
  root.innerHTML = "";
  const header = new Header({ title: "Saved Queries" });
  root.appendChild(header.root);
  const list = document.createElement("div");
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "10px";
  list.style.padding = "16px";
  root.appendChild(list);
  const items = loadSavedQueries();
  items.forEach((item) => {
    const row = document.createElement("div");
    row.className = "list-row";
    row.textContent = item.name;
    row.onclick = () => navigate(`/?query=${encodeURIComponent(item.query)}`);
    list.appendChild(row);
  });
  return root;
};

// ts/histogram.ts
class Histogram {
  root;
  canvas;
  ctx;
  data = [];
  zoom = 1;
  constructor() {
    this.root = document.createElement("div");
    this.root.style.overflowX = "auto";
    this.canvas = document.createElement("canvas");
    this.canvas.height = 200;
    this.canvas.width = 600;
    this.root.appendChild(this.canvas);
    const ctx = this.canvas.getContext("2d");
    if (!ctx)
      throw new Error("canvas 2d context not available");
    this.ctx = ctx;
    this.root.addEventListener("wheel", (e) => {
      e.preventDefault();
      const delta = e.deltaY < 0 ? 1.1 : 0.9;
      this.setZoom(this.zoom * delta);
    });
  }
  clear() {
    this.data = [];
    this.draw();
  }
  add(item) {
    this.data.push(item);
    this.draw();
  }
  setZoom(z) {
    this.zoom = Math.min(5, Math.max(0.5, z));
    this.draw();
  }
  draw() {
    const ctx = this.ctx;
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    if (this.data.length === 0)
      return;
    const max = Math.max(...this.data.map((d) => d.count));
    const barWidth = 10 * this.zoom;
    const width = Math.max(this.canvas.parentElement?.clientWidth || 600, barWidth * this.data.length);
    this.canvas.width = width;
    for (let i = 0;i < this.data.length; i++) {
      const item = this.data[i];
      const h = item.count / max * this.canvas.height;
      ctx.fillStyle = "#3B82F6";
      ctx.fillRect(i * barWidth, this.canvas.height - h, barWidth - 1, h);
    }
  }
}

// ts/logs.ts
var MAX_LOG_ENTRIES = 1e4;
var MESSAGE_TRUNCATE_LENGTH = 700;
var OBSERVER_THRESHOLD = 0.1;
var LOG_COLORS = {
  trace: "#6B7280",
  debug: "#3B82F6",
  info: "#10B981",
  warn: "#F59E0B",
  error: "#EF4444",
  fatal: "#8B5CF6"
};
var settingsSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-settings w-5 h-5"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path><circle cx="12" cy="12" r="3"></circle></svg>`;
var searchSvg = `<svg xmlns="http://www.w3.org/2000/svg"  viewBox="0 0 50 50" width="20px" height="20px"><path d="M 21 3 C 11.601563 3 4 10.601563 4 20 C 4 29.398438 11.601563 37 21 37 C 24.355469 37 27.460938 36.015625 30.09375 34.34375 L 42.375 46.625 L 46.625 42.375 L 34.5 30.28125 C 36.679688 27.421875 38 23.878906 38 20 C 38 10.601563 30.398438 3 21 3 Z M 21 7 C 28.199219 7 34 12.800781 34 20 C 34 27.199219 28.199219 33 21 33 C 13.800781 33 8 27.199219 8 20 C 8 12.800781 13.800781 7 21 7 Z"/></svg>`;
var formatTimestamp2 = (ts) => {
  const date = new Date(ts);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
};
var escapeHTML = (str) => {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
};
var truncateMessage = (msg) => msg.length > MESSAGE_TRUNCATE_LENGTH ? `${msg.slice(0, MESSAGE_TRUNCATE_LENGTH)}...` : msg;
var logsSearchPage = (args) => {
  const logIds = new Set;
  const logEntries = [];
  let moreRows = true;
  args.root.innerHTML = ``;
  const logsOptions = document.createElement("div");
  logsOptions.className = "page-header";
  args.root.appendChild(logsOptions);
  const searchTextarea = document.createElement("textarea");
  searchTextarea.className = "logs-search-bar";
  searchTextarea.placeholder = "Search logs (ctrl+enter to search)";
  searchTextarea.value = getQueryParam("query") || "";
  logsOptions.appendChild(searchTextarea);
  const optionsRightPanel = document.createElement("div");
  optionsRightPanel.className = "logs-options-right-panel";
  logsOptions.appendChild(optionsRightPanel);
  const settingsLink = document.createElement("a");
  settingsLink.className = "link";
  settingsLink.href = "/settings";
  settingsLink.innerHTML = settingsSvg;
  const saveButton = document.createElement("button");
  saveButton.textContent = "Save";
  saveButton.onclick = () => {
    const query = searchTextarea.value.trim();
    if (!query)
      return;
    const name = prompt("Query name", query);
    if (name)
      saveQuery({ name, query });
  };
  const searchButton = document.createElement("button");
  searchButton.innerHTML = searchSvg;
  const featuresList = new VList;
  const histogramToggle = document.createElement("label");
  histogramToggle.style.display = "flex";
  histogramToggle.style.alignItems = "center";
  const histogramCheckbox = document.createElement("input");
  histogramCheckbox.type = "checkbox";
  histogramToggle.appendChild(histogramCheckbox);
  histogramToggle.appendChild(document.createTextNode(" Show histogram"));
  featuresList.root.appendChild(histogramToggle);
  const featuresDropdown = new Collapsible({
    buttonText: "Options",
    content: featuresList
  });
  optionsRightPanel.append(settingsLink, searchButton);
  const histogramContainer = document.createElement("div");
  histogramContainer.style.display = "none";
  const histogram = new Histogram;
  histogramContainer.appendChild(histogram.root);
  args.root.appendChild(histogramContainer);
  let histStream = null;
  const startHistogram = () => {
    histogramContainer.style.display = "block";
    histogram.clear();
    const params = new URLSearchParams;
    if (searchTextarea.value)
      params.set("query", searchTextarea.value);
    params.set("bucketSecs", "60");
    params.set("tzOffset", new Date().getTimezoneOffset().toString());
    const url = new URL("/api/v1/logs/histogram", window.location.origin);
    url.search = params.toString();
    const es = new EventSource(url);
    es.onmessage = (ev) => {
      const item = JSON.parse(ev.data);
      histogram.add(item);
    };
    es.onerror = () => es.close();
    histStream = () => es.close();
  };
  const stopHistogram = () => {
    if (histStream)
      histStream();
    histStream = null;
    histogramContainer.style.display = "none";
    histogram.clear();
  };
  histogramCheckbox.onchange = () => {
    if (histogramCheckbox.checked)
      startHistogram();
    else
      stopHistogram();
  };
  const logsList = document.createElement("div");
  logsList.className = "logs-list";
  args.root.appendChild(logsList);
  const loadingIndicator = document.createElement("div");
  loadingIndicator.style.height = "50px";
  args.root.appendChild(loadingIndicator);
  let debounce;
  let pendingLogs = [];
  const addLogs = (log) => {
    pendingLogs.push(log);
    if (debounce)
      return;
    debounce = setTimeout(() => {
      const newEntries = pendingLogs.filter((entry) => !logIds.has(entry.id));
      newEntries.forEach((entry) => {
        logIds.add(entry.id);
        logEntries.push(entry);
      });
      logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
      if (logEntries.length > MAX_LOG_ENTRIES && args.root.scrollTop === 0) {
        const removed = logEntries.splice(MAX_LOG_ENTRIES);
        removed.forEach((r) => logIds.delete(r.id));
      }
      logsList.innerHTML = logEntries.map((entry) => `
				<div class="list-row">
					<div>
						${formatTimestamp2(entry.timestamp)} 
						<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
						${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
					</div>
					<div class="logs-list-row-msg">
						<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
					</div>
				</div>
			`).join("");
      document.querySelectorAll(".msg-summary").forEach((el, key) => {
        el.addEventListener("click", () => {
          const entry = logEntries[key];
          const isTruncated = entry.msg.length > MESSAGE_TRUNCATE_LENGTH;
          if (!isTruncated) {
            return;
          }
          showModal({
            title: "Log Message",
            content: formatLogMsg(entry.msg),
            footer: []
          });
        });
      });
      pendingLogs = [];
      debounce = null;
    }, 100);
  };
  const clearLogs = () => {
    logEntries.length = 0;
    logIds.clear();
    logsList.innerHTML = "";
  };
  let currentStream = null;
  const loadingIndicatorVisible = () => {
    const rect = loadingIndicator.getBoundingClientRect();
    return rect.top < window.innerHeight && rect.bottom >= 0;
  };
  let streamRowsCount = 0;
  let lastQuery = "";
  let lastEndDate = null;
  const queryLogs = async (clear) => {
    const query = searchTextarea.value;
    if (query !== lastQuery) {
      const error = await args.validateQuery(query);
      if (error) {
        removeQueryParam("query");
        logsList.innerHTML = "";
        loadingIndicator.innerHTML = `<div style="color: red">${error}</div>`;
        return;
      }
    }
    lastQuery = query;
    if (query)
      setQueryParam("query", query);
    else
      removeQueryParam("query");
    loadingIndicator.textContent = "Loading...";
    let endDate;
    if (logEntries.length > 0)
      endDate = logEntries[logEntries.length - 1].timestamp;
    if (lastEndDate !== null && endDate === lastEndDate)
      return;
    lastEndDate = endDate;
    console.log("endDate", endDate);
    if (clear)
      clearLogs();
    if (histogramCheckbox.checked) {
      stopHistogram();
      startHistogram();
    }
    if (currentStream)
      currentStream();
    currentStream = args.streamLogs({ query, count: 200, endDate }, (log) => {
      streamRowsCount++;
      addLogs(log);
    }, () => {
      currentStream = null;
      loadingIndicator.textContent = "";
      if (streamRowsCount === 0) {
        loadingIndicator.textContent = logEntries.length === 0 ? "No logs found" : "No more logs";
        return;
      }
      streamRowsCount = 0;
      if (loadingIndicatorVisible())
        queryLogs();
    });
  };
  searchTextarea.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      queryLogs(true);
    }
  });
  searchButton.addEventListener("click", () => queryLogs(true));
  const observer = new IntersectionObserver((entries) => {
    if (!entries[0].isIntersecting)
      return;
    queryLogs();
  }, { threshold: OBSERVER_THRESHOLD });
  observer.observe(loadingIndicator);
};

// ts/logtable-test.ts
function logline(length, linebreaks) {
  let line = "";
  for (let i = 0;i < length; i++) {
    line += String.fromCharCode(65 + Math.floor(Math.random() * 26));
  }
  for (let i = 0;i < linebreaks; i++) {
    const idx = Math.floor(Math.random() * (line.length + 1));
    line = line.slice(0, idx) + `
` + line.slice(idx);
  }
  return line;
}
function randomLogline(len) {
  const linebreaks = Math.floor(Math.random() * 10);
  return logline(len, linebreaks);
}
var createRandomJson = (totalPropsCount, maxDepth = 5) => {
  const root = {};
  let createdCount = 0;
  const queue = [];
  queue.push({ obj: root, depth: 0 });
  while (queue.length > 0 && createdCount < totalPropsCount) {
    const { obj, depth } = queue.shift();
    const remaining = totalPropsCount - createdCount;
    const numProps = Math.floor(Math.random() * Math.min(remaining, 10)) + 1;
    for (let i = 0;i < numProps && createdCount < totalPropsCount; i++) {
      const key = `key${createdCount}`;
      if (depth < maxDepth && Math.random() > 0.5) {
        const nestedObj = {};
        obj[key] = nestedObj;
        createdCount++;
        queue.push({ obj: nestedObj, depth: depth + 1 });
      } else {
        obj[key] = `value${createdCount}`;
        createdCount++;
      }
    }
  }
  return root;
};
var createRandomXml = (totalNodesCount, maxDepth = 5) => {
  const root = { tag: "root", children: [] };
  let createdCount = 0;
  const queue = [
    { node: root, depth: 0 }
  ];
  while (queue.length > 0 && createdCount < totalNodesCount) {
    const { node, depth } = queue.shift();
    const remaining = totalNodesCount - createdCount;
    const numChildren = Math.floor(Math.random() * Math.min(remaining, 10)) + 1;
    node.children = node.children || [];
    for (let i = 0;i < numChildren && createdCount < totalNodesCount; i++) {
      const tagName = `element${createdCount}`;
      if (depth < maxDepth && Math.random() > 0.5) {
        const childNode = { tag: tagName, children: [] };
        node.children.push(childNode);
        createdCount++;
        queue.push({ node: childNode, depth: depth + 1 });
      } else {
        const childNode = {
          tag: tagName,
          text: `value${createdCount}`
        };
        node.children.push(childNode);
        createdCount++;
      }
    }
  }
  const nodeToXml = (node) => {
    if (node.children && node.children.length > 0) {
      const childrenXml = node.children.map((child) => nodeToXml(child)).join("");
      return `<${node.tag}>${childrenXml}</${node.tag}>`;
    } else if (node.text !== undefined) {
      return `<${node.tag}>${node.text}</${node.tag}>`;
    } else {
      return `<${node.tag}/>`;
    }
  };
  return nodeToXml(root);
};
var logtableTest = (root) => {
  logsSearchPage({
    root,
    streamLogs: (args, onNewLog, onEnd) => {
      onNewLog({
        id: `${Date.now()}-text`,
        timestamp: new Date().toISOString(),
        level: "debug",
        props: [],
        msg: `Streamed log: ${randomLogline(1e5)}`
      });
      const randomPropsCount = Math.floor(Math.random() * 50) + 1;
      const randomPropsObject = createRandomJson(700);
      onNewLog({
        id: `${Date.now()}-json`,
        timestamp: new Date().toISOString(),
        level: "debug",
        props: [],
        msg: `JSON ${JSON.stringify(randomPropsObject)}`
      });
      const randomXml = createRandomXml(1000);
      onNewLog({
        id: `${Date.now()}-xml`,
        timestamp: new Date().toISOString(),
        level: "debug",
        props: [],
        msg: `XML ${randomXml}`
      });
      onEnd();
      return () => {};
    },
    validateQuery: async (query) => {
      return null;
    }
  });
  return root;
};

// ts/main-page.ts
var mainPage = (root) => {
  let query = getQueryParam("query") || "";
  let isStreaming = getQueryParam("stream") === "true";
  logsSearchPage({
    root,
    streamLogs: (args, onNewLog, onEnd) => {
      const streamQuery = new URLSearchParams;
      if (args.query)
        streamQuery.append("query", args.query);
      if (args.count)
        streamQuery.append("count", args.count.toString());
      if (args.endDate)
        streamQuery.append("endDate", args.endDate);
      streamQuery.append("tzOffset", new Date().getTimezoneOffset().toString());
      const streamUrl = new URL("/api/logs", window.location.origin);
      streamUrl.search = streamQuery.toString();
      const eventSource = new EventSource(streamUrl);
      eventSource.onmessage = (event) => {
        const data = JSON.parse(event.data);
        onNewLog(data);
      };
      eventSource.onerror = (event) => {
        console.log("eventSource.onerror", event);
        eventSource.close();
        onEnd();
      };
      return () => eventSource.close();
    },
    validateQuery: async (query2) => {
      let res = await fetch(`/api/v1/validate_query?query=${encodeURIComponent(query2)}`);
      if (res.status === 200)
        return null;
      return res.text();
    }
  });
  return root;
};

// ts/pivot.ts
var PivotPage = (root) => {
  const fakeData = [
    {
      logLevel: "Info",
      deviceId: "Device1",
      message: "Started process",
      timestamp: 1610000000000
    },
    {
      logLevel: "Error",
      deviceId: "Device2",
      message: "Failed to load module",
      timestamp: 1610000001000
    },
    {
      logLevel: "Warning",
      deviceId: "Device1",
      message: "Memory usage high",
      timestamp: 1610000002000
    },
    {
      logLevel: "Info",
      deviceId: "Device3",
      message: "Process completed",
      timestamp: 1610000003000
    },
    {
      logLevel: "Error",
      deviceId: "Device1",
      message: "Unhandled exception",
      timestamp: 1610000004000
    },
    {
      logLevel: "Debug",
      deviceId: "Device2",
      message: "Debugging info",
      timestamp: 1610000005000
    }
  ];
  const availableFields = ["logLevel", "deviceId", "timestamp", "message"];
  const container = document.createElement("div");
  container.style.display = "flex";
  container.style.flexDirection = "column";
  container.style.gap = "16px";
  container.style.fontFamily = "Arial, sans-serif";
  const configureButton = document.createElement("button");
  configureButton.textContent = "Configure Fields";
  configureButton.style.width = "150px";
  configureButton.style.padding = "8px";
  configureButton.style.cursor = "pointer";
  container.appendChild(configureButton);
  const modalOverlay = document.createElement("div");
  modalOverlay.style.position = "fixed";
  modalOverlay.style.top = "0";
  modalOverlay.style.left = "0";
  modalOverlay.style.width = "100%";
  modalOverlay.style.height = "100%";
  modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)";
  modalOverlay.style.display = "none";
  modalOverlay.style.justifyContent = "center";
  modalOverlay.style.alignItems = "center";
  modalOverlay.style.zIndex = "9999";
  const modalContent = document.createElement("div");
  modalContent.style.background = "#fff";
  modalContent.style.padding = "16px";
  modalContent.style.borderRadius = "4px";
  modalContent.style.minWidth = "200px";
  const modalTitle = document.createElement("h3");
  modalTitle.textContent = "Drag a Field";
  modalContent.appendChild(modalTitle);
  const closeModalBtn = document.createElement("button");
  closeModalBtn.textContent = "Close";
  closeModalBtn.style.marginBottom = "8px";
  closeModalBtn.addEventListener("click", () => {
    modalOverlay.style.display = "none";
  });
  modalContent.appendChild(closeModalBtn);
  availableFields.forEach((field) => {
    const fieldDiv = document.createElement("div");
    fieldDiv.textContent = field;
    fieldDiv.draggable = true;
    fieldDiv.style.border = "1px solid #ccc";
    fieldDiv.style.padding = "4px 8px";
    fieldDiv.style.margin = "4px 0";
    fieldDiv.style.cursor = "move";
    fieldDiv.style.backgroundColor = "#f9f9f9";
    fieldDiv.addEventListener("dragstart", (event) => {
      event.dataTransfer?.setData("text/plain", field);
      event.dataTransfer.effectAllowed = "move";
      fieldDiv.style.opacity = "0.5";
    });
    fieldDiv.addEventListener("dragend", () => {
      fieldDiv.style.opacity = "1";
    });
    modalContent.appendChild(fieldDiv);
  });
  modalOverlay.appendChild(modalContent);
  document.body.appendChild(modalOverlay);
  configureButton.addEventListener("click", () => {
    const hello = document.createElement("h1");
    hello.textContent = "Hello";
  });
  const dropZone = document.createElement("div");
  dropZone.innerHTML = "<h3>Drop a field here to group by</h3>";
  dropZone.style.border = "2px dashed #ccc";
  dropZone.style.padding = "16px";
  dropZone.style.margin = "16px 0";
  dropZone.style.textAlign = "center";
  dropZone.style.minHeight = "50px";
  container.appendChild(dropZone);
  let currentGroupField = "logLevel";
  const renderPivotTable = (groupField) => {
    const existingTable = container.querySelector("table");
    if (existingTable) {
      container.removeChild(existingTable);
    }
    const pivotResult = fakeData.reduce((acc, entry) => {
      const key = entry[groupField];
      acc[key] = (acc[key] || 0) + 1;
      return acc;
    }, {});
    const table = document.createElement("table");
    table.style.borderCollapse = "collapse";
    table.style.width = "100%";
    const thead = document.createElement("thead");
    const headerRow = document.createElement("tr");
    const colHeader = document.createElement("th");
    colHeader.textContent = groupField;
    colHeader.style.border = "1px solid #ccc";
    colHeader.style.padding = "8px";
    colHeader.style.background = "#f9f9f9";
    headerRow.appendChild(colHeader);
    const countHeader = document.createElement("th");
    countHeader.textContent = "Count";
    countHeader.style.border = "1px solid #ccc";
    countHeader.style.padding = "8px";
    countHeader.style.background = "#f9f9f9";
    headerRow.appendChild(countHeader);
    thead.appendChild(headerRow);
    table.appendChild(thead);
    const tbody = document.createElement("tbody");
    Object.entries(pivotResult).forEach(([key, count]) => {
      const row = document.createElement("tr");
      const keyTd = document.createElement("td");
      keyTd.textContent = key;
      keyTd.style.border = "1px solid #ccc";
      keyTd.style.padding = "8px";
      row.appendChild(keyTd);
      const countTd = document.createElement("td");
      countTd.textContent = count.toString();
      countTd.style.border = "1px solid #ccc";
      countTd.style.padding = "8px";
      row.appendChild(countTd);
      tbody.appendChild(row);
    });
    table.appendChild(tbody);
    container.appendChild(table);
  };
  dropZone.addEventListener("dragover", (event) => {
    event.preventDefault();
    dropZone.style.backgroundColor = "#eef";
  });
  dropZone.addEventListener("dragleave", () => {
    dropZone.style.backgroundColor = "";
  });
  dropZone.addEventListener("drop", (event) => {
    event.preventDefault();
    dropZone.style.backgroundColor = "";
    const field = event.dataTransfer?.getData("text/plain");
    if (field && availableFields.includes(field)) {
      currentGroupField = field;
      dropZone.innerHTML = `<h3>Grouping by: ${field}</h3>`;
      renderPivotTable(currentGroupField);
      modalOverlay.style.display = "none";
    }
  });
  renderPivotTable(currentGroupField);
  root.appendChild(container);
};

// ts/segment-page.ts
var fetchSegments = async (end) => {
  const url = new URL("/api/segments", window.location.origin);
  url.searchParams.set("end", end.toISOString());
  const res = await fetch(url.toString()).then((res2) => res2.json());
  return res;
};
var segmentsPage = async (root) => {
  const segementsMetadata = await fetch("/api/segment/metadata").then((res) => res.json());
  const compressionRatio = segementsMetadata.compressedSize / segementsMetadata.originalSize * 100;
  const averageCompressedLogSize = segementsMetadata.compressedSize / segementsMetadata.logsCount;
  const averageOriginalLogSize = segementsMetadata.originalSize / segementsMetadata.logsCount;
  const metadata = new KeyValueTable([
    {
      key: "Total segments",
      value: formatNumber(segementsMetadata.segmentCount)
    },
    {
      key: "Total original size",
      value: formatBytes(segementsMetadata.originalSize)
    },
    {
      key: "Total compressed size",
      value: formatBytes(segementsMetadata.compressedSize)
    },
    {
      key: "Total logs count",
      value: formatNumber(segementsMetadata.logsCount)
    },
    { key: "Compression ratio", value: compressionRatio.toFixed(2) + "%" },
    {
      key: "Average compressed log size",
      value: formatBytes(averageCompressedLogSize)
    },
    {
      key: "Average original log size",
      value: formatBytes(averageOriginalLogSize)
    },
    {
      key: "Average logs per segment",
      value: formatNumber(segementsMetadata.logsCount / segementsMetadata.segmentCount)
    },
    {
      key: "Average segment size",
      value: formatBytes(segementsMetadata.originalSize / segementsMetadata.segmentCount)
    }
  ]);
  metadata.root.style.whiteSpace = "nowrap";
  const metadataCollapsible = new Collapsible({
    buttonText: "Metadata",
    content: metadata
  });
  const header = new Header({
    title: "Segments",
    rightSide: metadataCollapsible
  });
  root.add(header);
  const segmentList = new WrapList;
  const infiniteScroll = new InfiniteScroll({
    container: segmentList
  });
  root.add(infiniteScroll);
  let endDate = new Date;
  infiniteScroll.onLoadMore = async () => {
    console.log("loadMore");
    const segments = await fetchSegments(endDate);
    endDate = new Date(segments[segments.length - 1].lastTimestamp);
    for (const segment of segments) {
      const table = new KeyValueTable([
        {
          key: "Segment ID",
          value: segment.id.toString(),
          href: `/segment/${segment.id}`
        },
        {
          key: "First timestamp",
          value: formatTimestamp(segment.firstTimestamp)
        },
        {
          key: "Last timestamp",
          value: formatTimestamp(segment.lastTimestamp)
        },
        {
          key: "Original size",
          value: formatBytes(segment.originalSize)
        },
        {
          key: "Compressed size",
          value: formatBytes(segment.compressedSize)
        },
        { key: "Logs count", value: formatNumber(segment.logsCount) },
        {
          key: "Compression ratio",
          value: (segment.compressedSize / segment.originalSize * 100).toFixed(2) + "%"
        }
      ]);
      segmentList.add(table);
    }
  };
};
var segmentPage = async (root, segmentId) => {
  const segment = await fetch(`/api/v1/segment/${segmentId}`).then((res) => res.json());
  const props = await fetch(`/api/v1/segment/${segmentId}/props`).then((res) => res.json());
  const totalOriginalSize = segment.originalSize;
  const totalCompressedSize = segment.compressedSize;
  const totalLogsCount = segment.logsCount;
  const compressRatio = totalCompressedSize / totalOriginalSize * 100;
  root.innerHTML = `
		<div class="page-header">
			<h1 style="flex-grow: 1">Segment ${segmentId}</h1>
			<div class="summary">
				<div><strong>First timestamp:</strong> ${formatTimestamp(segment.firstTimestamp)}</div>
				<div><strong>Last timestamp:</strong> ${formatTimestamp(segment.lastTimestamp)}</div>
				<div><strong>Total original size:</strong> ${formatBytes(totalOriginalSize)}</div>
				<div><strong>Total compressed size:</strong> ${formatBytes(totalCompressedSize)}</div>
				<div><strong>Total logs count:</strong> ${formatNumber(totalLogsCount)}</div>
				<div><strong>Compression ratio:</strong> ${compressRatio.toFixed(2)}%</div>
			</div>
		</div>
		<div style="display: flex; flex-wrap: wrap; gap: 10px; margin: 10px">
			${props.map((prop) => `
				<div class="list-row">
					<div class="table-cell"><strong>Key:</strong> ${prop.key}</div>
					<div class="table-cell"><strong>Value:</strong> ${prop.value}</div>
				</div>
			`).join("")}
		</div>
	`;
};

// ts/settings.ts
class LinkList extends UiComponent {
  constructor(links) {
    super(document.createElement("div"));
    this.root.style.display = "flex";
    this.root.style.flexWrap = "wrap";
    this.root.style.gap = "10px";
    for (const link of links) {
      const item = document.createElement("div");
      item.className = "list-row";
      item.style.padding = "30px";
      this.root.appendChild(item);
      const linkElement = document.createElement("a");
      linkElement.href = link.href;
      linkElement.innerText = link.text;
      item.appendChild(linkElement);
    }
  }
}
var settingsPage = (root) => {
  root.root.innerHTML = "";
  const linkList = new LinkList([
    { href: "/logs", text: "Logs" },
    { href: "/devices", text: "Devices" },
    { href: "/segments", text: "Segments" },
    { href: "/queries", text: "Saved Queries" }
  ]);
  root.add(linkList);
};

// ts/app.ts
window.onload = () => {
  const body = document.querySelector("body");
  if (!body) {
    throw new Error("No body element found");
  }
  const container = new Container(body);
  routes({
    "/tests/logs": () => logtableTest(body),
    "/settings": () => settingsPage(container),
    "/devices": () => devicesPage(body),
    "/segments": () => segmentsPage(container),
    "/queries": () => queriesPage(body),
    "/segment/:segmentId": (params) => segmentPage(body, params.segmentId),
    "/pivot": () => PivotPage(body),
    "/*": () => mainPage(body)
  });
};
