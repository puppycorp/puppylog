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
    this.root.className = "summary";
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
  modalOverlay.className = "modal-overlay";
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
  return () => modalOverlay.remove();
};

// ts/navbar.ts
class Navbar extends HList {
  logsLink;
  devicesLink;
  segmentsLink;
  leftItems;
  constructor(args) {
    super();
    this.root.classList.add("page-header");
    this.root.style.gap = "8px";
    this.logsLink = document.createElement("a");
    this.logsLink.textContent = "Logs";
    this.logsLink.href = "/logs";
    this.logsLink.classList.add("link");
    this.devicesLink = document.createElement("a");
    this.devicesLink.textContent = "Devices";
    this.devicesLink.href = "/devices";
    this.devicesLink.classList.add("link");
    this.segmentsLink = document.createElement("a");
    this.segmentsLink.textContent = "Segments";
    this.segmentsLink.href = "/segments";
    this.segmentsLink.classList.add("link");
    const currentPath = window.location.pathname;
    [
      { link: this.logsLink, path: "/logs" },
      { link: this.devicesLink, path: "/devices" },
      { link: this.segmentsLink, path: "/segments" }
    ].forEach(({ link, path }) => {
      if (currentPath === path || currentPath.startsWith(path + "/")) {
        link.classList.add("active");
      }
    });
    this.leftItems = [this.logsLink, this.devicesLink, this.segmentsLink];
    this.setRight(args?.right);
  }
  setRight(right) {
    while (this.root.firstChild) {
      this.root.removeChild(this.root.firstChild);
    }
    if (right && right.length) {
      const spacer = document.createElement("div");
      spacer.style.flex = "1";
      const rightEls = right.map((item) => item instanceof HTMLElement ? item : item.root);
      this.add(...this.leftItems, spacer, ...rightEls);
    } else {
      this.add(...this.leftItems);
    }
  }
}

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
    const idLabel = document.createElement("strong");
    idLabel.textContent = "ID: ";
    idCell.appendChild(idLabel);
    const idLink = document.createElement("a");
    idLink.textContent = device.id;
    idLink.href = `/device/${encodeURIComponent(device.id)}`;
    idLink.classList.add("link");
    idLink.onclick = (event) => {
      event.preventDefault();
      event.stopPropagation();
      navigate(`/device/${encodeURIComponent(device.id)}`);
    };
    idCell.appendChild(idLink);
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
var devicesPage = async (root) => {
  root.innerHTML = "";
  const page = new Container(root);
  const navbar = new Navbar;
  page.add(navbar);
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
  const metadataTable = new KeyValueTable([
    { key: "Total devices", value: formatNumber(devices.length) },
    { key: "Total logs count", value: formatNumber(totalLogsCount) },
    { key: "Total logs size", value: formatBytes(totalLogsSize) },
    { key: "Average log size", value: formatBytes(averageLogSize) },
    { key: "Logs per second", value: totalLogsPerSecond.toFixed(2) }
  ]);
  metadataTable.root.style.whiteSpace = "nowrap";
  const metadataCollapsible = new Collapsible({
    buttonText: "Metadata",
    content: metadataTable
  });
  navbar.setRight([metadataCollapsible]);
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
  page.add(navbar, searchOptions, devicesList);
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
};

// ts/device-page.ts
var SEGMENTS_PAGE_SIZE = 20;
var fetchDevice = async (deviceId) => {
  const response = await fetch(`/api/v1/device/${encodeURIComponent(deviceId)}`);
  if (response.status === 404) {
    throw new Error("Device not found");
  }
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return await response.json();
};
var fetchDeviceSegments = async (deviceId, options) => {
  const url = new URL("/api/segments", window.location.origin);
  url.searchParams.append("device_ids[]", deviceId);
  const count = options?.count ?? SEGMENTS_PAGE_SIZE;
  url.searchParams.set("count", count.toString());
  url.searchParams.set("sort", "desc");
  if (options?.end)
    url.searchParams.set("end", options.end.toISOString());
  const response = await fetch(url.toString());
  if (!response.ok)
    throw new Error(await response.text());
  return await response.json();
};
var downloadSegmentLogs = async (segmentId, button, statusEl) => {
  const previousText = button.root.textContent || "Download logs";
  setStatus(statusEl, "", "idle");
  button.root.disabled = true;
  button.root.textContent = "Downloading...";
  try {
    const res = await fetch(`/api/v1/segment/${segmentId}/logs.txt`);
    if (!res.ok)
      throw new Error(await res.text());
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `segment-${segmentId}.txt`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  } catch (error) {
    setStatus(statusEl, error instanceof Error ? error.message || "Failed to download segment logs." : "Failed to download segment logs.", "error");
  } finally {
    button.root.disabled = false;
    button.root.textContent = previousText;
  }
};
var createSegmentCard = (segment, statusEl) => {
  const table = new KeyValueTable([
    {
      key: "Segment",
      value: `#${segment.id}`,
      href: `/segment/${segment.id}`
    },
    {
      key: "First timestamp",
      value: formatDate(segment.firstTimestamp)
    },
    {
      key: "Last timestamp",
      value: formatDate(segment.lastTimestamp)
    },
    {
      key: "Logs count",
      value: formatNumber(segment.logsCount)
    },
    {
      key: "Original size",
      value: formatBytes(segment.originalSize)
    },
    {
      key: "Compressed size",
      value: formatBytes(segment.compressedSize)
    }
  ]);
  const segmentCard = new VList({
    style: { gap: "8px" }
  });
  segmentCard.root.classList.add("summary");
  segmentCard.add(table);
  const downloadButton = new Button({
    text: "Download logs (.txt)"
  });
  downloadButton.onClick = () => downloadSegmentLogs(segment.id, downloadButton, statusEl);
  segmentCard.add(downloadButton);
  return segmentCard;
};
var updateDeviceSettings = async (deviceId, payload) => {
  const response = await fetch(`/api/v1/device/${encodeURIComponent(deviceId)}/settings`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
};
var updateDeviceMetadata = async (deviceId, props) => {
  const response = await fetch(`/api/v1/device/${encodeURIComponent(deviceId)}/metadata`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(props)
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
};
var createSection = (title) => {
  const section = new VList({
    style: {
      gap: "12px"
    }
  });
  section.root.classList.add("summary");
  const heading = document.createElement("h2");
  heading.textContent = title;
  heading.style.margin = "0";
  section.root.appendChild(heading);
  return section;
};
var formatDate = (value) => {
  if (!value)
    return "Never";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? "Never" : parsed.toLocaleString();
};
var logsPerSecond = (device) => {
  if (!device.createdAt || !device.lastUploadAt)
    return 0;
  const createdAt = new Date(device.createdAt);
  const lastUpload = new Date(device.lastUploadAt);
  const diffSeconds = (lastUpload.getTime() - createdAt.getTime()) / 1000;
  if (!Number.isFinite(diffSeconds) || diffSeconds <= 0)
    return 0;
  return device.logsCount / diffSeconds;
};
var setStatus = (element, message, type) => {
  element.textContent = message;
  switch (type) {
    case "info":
      element.style.color = "#047857";
      break;
    case "error":
      element.style.color = "#b91c1c";
      break;
    default:
      element.style.color = "";
  }
};
var devicePage = async (root, deviceId) => {
  root.innerHTML = "";
  const page = new Container(root);
  const navbar = new Navbar;
  page.add(navbar);
  const content = new VList({
    style: {
      gap: "16px"
    }
  });
  content.root.style.padding = "16px";
  content.root.style.maxWidth = "960px";
  content.root.style.margin = "0 auto";
  page.add(content);
  const title = document.createElement("h1");
  title.textContent = `Device ${deviceId}`;
  title.style.margin = "0";
  content.root.appendChild(title);
  const loading = document.createElement("div");
  loading.className = "logs-loading-indicator";
  loading.textContent = "Loading device...";
  content.root.appendChild(loading);
  let device;
  try {
    device = await fetchDevice(deviceId);
  } catch (error) {
    loading.textContent = error instanceof Error ? error.message || "Failed to load device" : "Failed to load device";
    loading.classList.remove("logs-loading-indicator");
    loading.style.color = "#b91c1c";
    return;
  }
  content.root.removeChild(loading);
  title.textContent = `Device ${device.id}`;
  const stats = new KeyValueTable([
    { key: "Created", value: formatDate(device.createdAt) },
    { key: "Last upload", value: formatDate(device.lastUploadAt) },
    { key: "Logs count", value: formatNumber(device.logsCount) },
    { key: "Logs size", value: formatBytes(device.logsSize) },
    {
      key: "Average log size",
      value: device.logsCount === 0 ? "0 Bytes" : formatBytes(device.logsSize / device.logsCount)
    },
    {
      key: "Logs per second",
      value: logsPerSecond(device).toFixed(2)
    }
  ]);
  content.add(stats);
  const settingsSection = createSection("Settings");
  content.add(settingsSection);
  const filterLevelSelect = new SelectGroup({
    label: "Filter level",
    value: device.filterLevel,
    options: levels.map((level) => ({
      text: level,
      value: level
    }))
  });
  const sendLogsSelect = new SelectGroup({
    label: "Send logs",
    value: device.sendLogs ? "true" : "false",
    options: [
      { text: "Yes", value: "true" },
      { text: "No", value: "false" }
    ]
  });
  const sendIntervalInput = new TextInput({
    label: "Send interval (seconds)",
    value: device.sendInterval.toString()
  });
  const sendIntervalInputEl = sendIntervalInput.root.querySelector("input");
  if (sendIntervalInputEl) {
    sendIntervalInputEl.type = "number";
    sendIntervalInputEl.min = "0";
  }
  const saveSettingsButton = new Button({ text: "Save settings" });
  saveSettingsButton.root.disabled = true;
  const settingsStatus = document.createElement("div");
  setStatus(settingsStatus, "", "idle");
  let settingsDirty = false;
  const markSettingsDirty = () => {
    if (!settingsDirty) {
      saveSettingsButton.root.disabled = false;
      settingsDirty = true;
    }
    setStatus(settingsStatus, "", "idle");
  };
  filterLevelSelect.onChange = () => markSettingsDirty();
  sendLogsSelect.onChange = () => markSettingsDirty();
  if (sendIntervalInputEl) {
    sendIntervalInputEl.oninput = () => markSettingsDirty();
  }
  saveSettingsButton.onClick = async () => {
    if (!settingsDirty)
      return;
    const interval = sendIntervalInputEl ? parseInt(sendIntervalInputEl.value, 10) : device.sendInterval;
    if (!Number.isFinite(interval) || interval < 0) {
      setStatus(settingsStatus, "Send interval must be a non-negative number", "error");
      return;
    }
    saveSettingsButton.root.disabled = true;
    setStatus(settingsStatus, "Saving settings...", "idle");
    try {
      await updateDeviceSettings(device.id, {
        sendLogs: sendLogsSelect.value === "true",
        filterLevel: filterLevelSelect.value,
        sendInterval: interval
      });
      device.sendLogs = sendLogsSelect.value === "true";
      device.filterLevel = filterLevelSelect.value;
      device.sendInterval = interval;
      settingsDirty = false;
      setStatus(settingsStatus, "Settings saved", "info");
    } catch (error) {
      setStatus(settingsStatus, error instanceof Error ? error.message || "Failed to save settings" : "Failed to save settings", "error");
      saveSettingsButton.root.disabled = false;
    }
  };
  settingsSection.add(filterLevelSelect, sendLogsSelect, sendIntervalInput, saveSettingsButton);
  settingsSection.root.appendChild(settingsStatus);
  const metadataSection = createSection("Metadata");
  content.add(metadataSection);
  let props = device.props ? device.props.map((prop) => ({ ...prop })) : [];
  const propsList = new VList({
    style: {
      gap: "8px"
    }
  });
  const metadataStatus = document.createElement("div");
  setStatus(metadataStatus, "", "idle");
  const metadataSaveButton = new Button({ text: "Save metadata" });
  metadataSaveButton.root.disabled = true;
  let metadataDirty = false;
  const markMetadataDirty = () => {
    metadataDirty = true;
    metadataSaveButton.root.disabled = false;
    setStatus(metadataStatus, "", "idle");
  };
  const renderProps = () => {
    propsList.root.innerHTML = "";
    if (props.length === 0) {
      const empty = document.createElement("div");
      empty.textContent = "No metadata";
      empty.style.color = "#6b7280";
      propsList.root.appendChild(empty);
      return;
    }
    props.forEach((prop, index) => {
      const row = document.createElement("div");
      row.style.display = "flex";
      row.style.flexWrap = "wrap";
      row.style.gap = "8px";
      row.style.alignItems = "center";
      const keyInput = document.createElement("input");
      keyInput.type = "text";
      keyInput.placeholder = "Key";
      keyInput.value = prop.key;
      keyInput.oninput = () => {
        props[index].key = keyInput.value;
        markMetadataDirty();
      };
      const valueInput = document.createElement("input");
      valueInput.type = "text";
      valueInput.placeholder = "Value";
      valueInput.value = prop.value;
      valueInput.oninput = () => {
        props[index].value = valueInput.value;
        markMetadataDirty();
      };
      const removeButton = document.createElement("button");
      removeButton.textContent = "Remove";
      removeButton.onclick = () => {
        props.splice(index, 1);
        renderProps();
        markMetadataDirty();
      };
      row.append(keyInput, valueInput, removeButton);
      propsList.root.appendChild(row);
    });
  };
  renderProps();
  const addPropButton = new Button({ text: "Add property" });
  addPropButton.onClick = () => {
    props.push({ key: "", value: "" });
    renderProps();
    markMetadataDirty();
  };
  metadataSaveButton.onClick = async () => {
    if (!metadataDirty)
      return;
    const sanitized = props.map((prop) => ({ key: prop.key.trim(), value: prop.value.trim() })).filter((prop) => prop.key.length > 0);
    metadataSaveButton.root.disabled = true;
    setStatus(metadataStatus, "Saving metadata...", "idle");
    try {
      await updateDeviceMetadata(device.id, sanitized);
      device.props = sanitized;
      props = sanitized.map((prop) => ({ ...prop }));
      renderProps();
      metadataDirty = false;
      setStatus(metadataStatus, "Metadata saved", "info");
    } catch (error) {
      setStatus(metadataStatus, error instanceof Error ? error.message || "Failed to save metadata" : "Failed to save metadata", "error");
      metadataSaveButton.root.disabled = false;
    }
  };
  metadataSection.add(propsList, addPropButton, metadataSaveButton);
  metadataSection.root.appendChild(metadataStatus);
  const segmentsSection = createSection("Segments");
  const segmentsList = new VList({
    style: {
      gap: "12px"
    }
  });
  const segmentsStatus = document.createElement("div");
  setStatus(segmentsStatus, "Loading segments…", "idle");
  segmentsSection.add(segmentsList);
  const loadMoreButton = new Button({ text: "Load more segments" });
  loadMoreButton.root.style.alignSelf = "flex-start";
  loadMoreButton.root.style.display = "none";
  segmentsSection.add(loadMoreButton);
  segmentsSection.root.appendChild(segmentsStatus);
  content.add(segmentsSection);
  let segmentsEndDate = null;
  let segmentsExhausted = false;
  let segmentsLoading = false;
  const renderSegments = (segments) => {
    segments.forEach((segment) => {
      const card = createSegmentCard(segment, segmentsStatus);
      segmentsList.add(card);
    });
  };
  const updateLoadMoreVisibility = (lastBatchSize) => {
    if (segmentsExhausted || lastBatchSize < SEGMENTS_PAGE_SIZE) {
      loadMoreButton.root.style.display = "none";
    } else {
      loadMoreButton.root.style.display = "inline-flex";
      loadMoreButton.root.disabled = false;
      loadMoreButton.root.textContent = "Load more segments";
    }
  };
  const loadSegments = async (append) => {
    if (segmentsLoading)
      return;
    segmentsLoading = true;
    if (!append) {
      segmentsList.root.innerHTML = "";
      setStatus(segmentsStatus, "Loading segments…", "idle");
    } else {
      loadMoreButton.root.disabled = true;
      loadMoreButton.root.textContent = "Loading…";
    }
    try {
      const segments = await fetchDeviceSegments(device.id, {
        count: SEGMENTS_PAGE_SIZE,
        end: segmentsEndDate
      });
      if (segments.length === 0 && !append) {
        setStatus(segmentsStatus, "No segments for this device yet.", "idle");
        loadMoreButton.root.style.display = "none";
        segmentsExhausted = true;
      } else {
        setStatus(segmentsStatus, "", "idle");
        renderSegments(segments);
        if (segments.length > 0) {
          const last = segments[segments.length - 1];
          segmentsEndDate = new Date(last.lastTimestamp);
        }
        if (segments.length < SEGMENTS_PAGE_SIZE) {
          segmentsExhausted = true;
        }
        updateLoadMoreVisibility(segments.length);
      }
    } catch (error) {
      setStatus(segmentsStatus, error instanceof Error ? error.message || "Failed to load segments." : "Failed to load segments.", "error");
      loadMoreButton.root.style.display = "inline-flex";
      loadMoreButton.root.disabled = false;
      loadMoreButton.root.textContent = "Retry";
    } finally {
      segmentsLoading = false;
    }
  };
  loadMoreButton.onClick = () => loadSegments(true);
  await loadSegments(false);
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
var isFiniteNumber = (value) => typeof value === "number" && Number.isFinite(value);
var isSegmentProgressEvent = (value) => {
  if (typeof value !== "object" || value === null)
    return false;
  const event = value;
  if (event.type !== "segment")
    return false;
  if (!isFiniteNumber(event.segmentId))
    return false;
  if (typeof event.firstTimestamp !== "string")
    return false;
  if (typeof event.lastTimestamp !== "string")
    return false;
  if ("logsCount" in event && event.logsCount !== undefined && !isFiniteNumber(event.logsCount))
    return false;
  if ("deviceId" in event && event.deviceId !== undefined && event.deviceId !== null && typeof event.deviceId !== "string")
    return false;
  return true;
};
var isSearchProgressEvent = (value) => {
  if (typeof value !== "object" || value === null)
    return false;
  const event = value;
  if (event.type !== "stats")
    return false;
  if (!isFiniteNumber(event.processedLogs))
    return false;
  if ("logsPerSecond" in event && event.logsPerSecond !== undefined)
    return isFiniteNumber(event.logsPerSecond);
  if ("status" in event && event.status !== undefined && event.status !== null && typeof event.status !== "string")
    return false;
  return true;
};
var MAX_LOG_ENTRIES = 1e4;
var MESSAGE_TRUNCATE_LENGTH = 700;
var OBSERVER_THRESHOLD = 0.1;
var DEFAULT_DOWNLOAD_COUNT = 500;
var MAX_DOWNLOAD_COUNT = 50000;
var LOG_COLORS = {
  trace: "#6B7280",
  debug: "#3B82F6",
  info: "#10B981",
  warn: "#F59E0B",
  error: "#EF4444",
  fatal: "#8B5CF6"
};
var searchSvg = `<svg xmlns="http://www.w3.org/2000/svg"  viewBox="0 0 50 50" width="20px" height="20px"><path d="M 21 3 C 11.601563 3 4 10.601563 4 20 C 4 29.398438 11.601563 37 21 37 C 24.355469 37 27.460938 36.015625 30.09375 34.34375 L 42.375 46.625 L 46.625 42.375 L 34.5 30.28125 C 36.679688 27.421875 38 23.878906 38 20 C 38 10.601563 30.398438 3 21 3 Z M 21 7 C 28.199219 7 34 12.800781 34 20 C 34 27.199219 28.199219 33 21 33 C 13.800781 33 8 27.199219 8 20 C 8 12.800781 13.800781 7 21 7 Z"/></svg>`;
var formatTimestamp2 = (ts) => {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime()))
    return "unknown time";
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
};
var describeSegmentProgress = (progress) => {
  const start = formatTimestamp2(progress.firstTimestamp);
  const end = formatTimestamp2(progress.lastTimestamp);
  const device = progress.deviceId ? ` · ${progress.deviceId}` : "";
  const logs = progress.logsCount ? ` · ${progress.logsCount} logs` : "";
  return `Scanning segment ${progress.segmentId}${device} (${start} – ${end}${logs})`;
};
var describeSearchProgress = (progress) => {
  const processed = progress.processedLogs.toLocaleString();
  const speed = Number.isFinite(progress.logsPerSecond) ? progress.logsPerSecond : 0;
  const speedText = speed > 0 ? ` · ${speed.toFixed(1)} logs/sec` : "";
  const statusText = progress.status ? ` · ${progress.status}` : "";
  return `Processed ${processed} logs${speedText}${statusText}`;
};
var escapeHTML = (str) => {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
};
var truncateMessage = (msg) => msg.length > MESSAGE_TRUNCATE_LENGTH ? `${msg.slice(0, MESSAGE_TRUNCATE_LENGTH)}...` : msg;
var formatRawMessage = (msg) => escapeHTML(msg.replace(/[\r\n]+/g, " "));
var formatDownloadLine = (entry) => {
  const propsText = entry.props.length > 0 ? ` ${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}` : "";
  const msg = entry.msg.replace(/[\r\n]+/g, " ").trim();
  const msgText = msg ? ` ${msg}` : "";
  return `${formatTimestamp2(entry.timestamp)} ${entry.level.toUpperCase()}${propsText}${msgText}`;
};
var logsSearchPage = (args) => {
  const logIds = new Set;
  const logEntries = [];
  let logViewMode = "structured";
  let rawWrapEnabled = false;
  args.root.innerHTML = ``;
  const navbar = new Navbar;
  args.root.appendChild(navbar.root);
  const header = document.createElement("div");
  header.className = "page-header logs-header";
  args.root.appendChild(header);
  const headerControls = document.createElement("div");
  headerControls.className = "logs-header-controls";
  header.appendChild(headerControls);
  const searchTextarea = document.createElement("textarea");
  searchTextarea.className = "logs-search-bar";
  searchTextarea.placeholder = "Search logs (ctrl+enter to search)";
  searchTextarea.value = getQueryParam("query") || "";
  headerControls.appendChild(searchTextarea);
  const searchButton = document.createElement("button");
  searchButton.innerHTML = searchSvg;
  searchButton.setAttribute("aria-busy", "false");
  const stopButton = document.createElement("button");
  stopButton.textContent = "Stop";
  stopButton.disabled = true;
  stopButton.style.display = "none";
  const downloadButton = document.createElement("button");
  downloadButton.textContent = "Download";
  const actionBar = document.createElement("div");
  actionBar.className = "logs-action-bar";
  headerControls.appendChild(actionBar);
  const searchControls = document.createElement("div");
  searchControls.className = "logs-search-controls";
  searchControls.append(searchButton, stopButton, downloadButton);
  actionBar.appendChild(searchControls);
  const viewActions = document.createElement("div");
  viewActions.className = "logs-view-actions";
  actionBar.appendChild(viewActions);
  const viewToggleWrapper = document.createElement("label");
  viewToggleWrapper.className = "logs-view-toggle";
  viewToggleWrapper.style.display = "flex";
  viewToggleWrapper.style.alignItems = "center";
  viewToggleWrapper.style.gap = "8px";
  const viewToggleLabel = document.createElement("span");
  viewToggleLabel.textContent = "View";
  const viewToggle = document.createElement("select");
  const viewStructured = document.createElement("option");
  viewStructured.value = "structured";
  viewStructured.textContent = "Structured";
  const viewRaw = document.createElement("option");
  viewRaw.value = "raw";
  viewRaw.textContent = "Raw text";
  viewToggle.append(viewStructured, viewRaw);
  viewToggleWrapper.append(viewToggleLabel, viewToggle);
  viewActions.appendChild(viewToggleWrapper);
  const wrapToggleWrapper = document.createElement("label");
  wrapToggleWrapper.className = "logs-raw-wrap-toggle";
  wrapToggleWrapper.style.display = "none";
  wrapToggleWrapper.style.alignItems = "center";
  wrapToggleWrapper.style.gap = "6px";
  const wrapToggle = document.createElement("input");
  wrapToggle.type = "checkbox";
  const wrapToggleLabel = document.createElement("span");
  wrapToggleLabel.textContent = "Wrap raw logs";
  wrapToggleWrapper.append(wrapToggle, wrapToggleLabel);
  viewActions.appendChild(wrapToggleWrapper);
  wrapToggle.addEventListener("change", () => {
    rawWrapEnabled = wrapToggle.checked;
    renderLogs();
  });
  viewToggle.addEventListener("change", () => {
    logViewMode = viewToggle.value;
    const isRaw = logViewMode === "raw";
    wrapToggleWrapper.style.display = isRaw ? "flex" : "none";
    renderLogs();
  });
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
  const loadingIndicator = document.createElement("div");
  loadingIndicator.className = "logs-loading-indicator";
  loadingIndicator.style.display = "none";
  loadingIndicator.style.alignItems = "center";
  loadingIndicator.style.gap = "8px";
  loadingIndicator.style.padding = "4px 16px";
  loadingIndicator.style.fontSize = "12px";
  loadingIndicator.style.color = "#6b7280";
  loadingIndicator.style.justifyContent = "flex-start";
  const loadingSpinner = document.createElement("span");
  loadingSpinner.className = "logs-search-spinner";
  loadingSpinner.style.display = "none";
  const loadingText = document.createElement("span");
  loadingText.textContent = "";
  loadingIndicator.append(loadingSpinner, loadingText);
  header.appendChild(loadingIndicator);
  const setLoadingIndicator = (text, spinning, color) => {
    loadingText.textContent = text;
    loadingSpinner.style.display = spinning ? "inline-block" : "none";
    if (color)
      loadingText.style.color = color;
    else
      loadingText.style.color = "#6b7280";
    if (!text && !spinning) {
      loadingIndicator.style.display = "none";
    } else {
      loadingIndicator.style.display = "flex";
    }
  };
  let segmentStatus = "";
  let statsStatus = "";
  const updateProgressIndicator = () => {
    if (!segmentStatus && !statsStatus) {
      setLoadingIndicator("Searching…", true);
      return;
    }
    const parts = [];
    if (segmentStatus)
      parts.push(segmentStatus);
    if (statsStatus)
      parts.push(statsStatus);
    setLoadingIndicator(parts.join(" · "), true);
  };
  const logsList = document.createElement("div");
  logsList.className = "logs-list";
  args.root.appendChild(logsList);
  const fetchLogsForDownload = async (count) => {
    const query = searchTextarea.value.trim();
    if (query) {
      const validationError = await args.validateQuery(query);
      if (validationError)
        throw new Error(validationError);
    }
    const params = new URLSearchParams;
    if (query)
      params.set("query", query);
    params.set("count", Math.max(1, Math.min(MAX_DOWNLOAD_COUNT, count)).toString());
    params.set("tzOffset", new Date().getTimezoneOffset().toString());
    const res = await fetch(`/api/logs?${params.toString()}`, {
      headers: {
        Accept: "application/json"
      }
    });
    if (!res.ok) {
      const message = await res.text();
      throw new Error(message || "Failed to download logs");
    }
    return await res.json();
  };
  const saveLogsToFile = (entries) => {
    if (entries.length === 0)
      throw new Error("No logs matched the current query.");
    const content = entries.map((entry) => formatDownloadLine(entry)).join(`
`);
    const blob = new Blob([content], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    const filename = `puppylog-${entries.length}-logs-${timestamp}.log`;
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };
  const openDownloadModal = () => {
    const modalContent = document.createElement("div");
    modalContent.className = "logs-download-modal";
    const description = document.createElement("p");
    description.textContent = `Download up to ${MAX_DOWNLOAD_COUNT.toLocaleString()} logs that match the current query.`;
    const inputLabel = document.createElement("label");
    inputLabel.textContent = "Number of logs";
    const countInput = document.createElement("input");
    countInput.type = "number";
    countInput.min = "1";
    countInput.max = MAX_DOWNLOAD_COUNT.toString();
    countInput.value = DEFAULT_DOWNLOAD_COUNT.toString();
    countInput.inputMode = "numeric";
    const errorEl = document.createElement("div");
    errorEl.className = "logs-download-error";
    modalContent.append(description, inputLabel, countInput, errorEl);
    const cancelButton = new Button({ text: "Cancel" });
    const confirmButton = new Button({ text: "Download" });
    const closeModal = showModal({
      title: "Download logs",
      minWidth: 360,
      content: modalContent,
      footer: [cancelButton, confirmButton]
    });
    cancelButton.onClick = () => closeModal();
    confirmButton.onClick = async () => {
      errorEl.textContent = "";
      let desired = Number(countInput.value);
      if (!Number.isFinite(desired) || desired <= 0) {
        errorEl.textContent = "Enter a positive number.";
        return;
      }
      if (desired > MAX_DOWNLOAD_COUNT)
        desired = MAX_DOWNLOAD_COUNT;
      confirmButton.root.disabled = true;
      confirmButton.root.textContent = "Downloading…";
      try {
        const entries = await fetchLogsForDownload(desired);
        saveLogsToFile(entries);
        closeModal();
      } catch (err) {
        errorEl.textContent = err instanceof Error ? err.message : "Unable to download logs";
      } finally {
        confirmButton.root.disabled = false;
        confirmButton.root.textContent = "Download";
      }
    };
    countInput.select();
  };
  const scrollSentinel = document.createElement("div");
  scrollSentinel.style.height = "1px";
  scrollSentinel.style.width = "100%";
  scrollSentinel.style.marginTop = "4px";
  args.root.appendChild(scrollSentinel);
  let debounce;
  let pendingLogs = [];
  const renderLogs = () => {
    logsList.classList.toggle("logs-list-raw", logViewMode === "raw");
    logsList.classList.toggle("logs-list-raw-wrap", logViewMode === "raw" && rawWrapEnabled);
    logsList.innerHTML = logEntries.map((entry, idx) => {
      if (logViewMode === "raw") {
        return `
				<div class="list-row logs-raw-row" data-entry-index="${idx}">
					<div>
						${formatTimestamp2(entry.timestamp)}
						<span style="color: ${LOG_COLORS[entry.level]}">${entry.level.toUpperCase()}</span>
						${formatRawMessage(entry.msg)}
					</div>
				</div>`;
      }
      return `
			<div class="list-row" data-entry-index="${idx}">
				<div>
					${formatTimestamp2(entry.timestamp)}
					<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
					${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
				</div>
				<div class="logs-list-row-msg">
					<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
				</div>
			</div>`;
    }).join("");
    if (logViewMode === "structured") {
      logsList.querySelectorAll(".msg-summary").forEach((el) => {
        el.addEventListener("click", () => {
          const parent = el.closest("[data-entry-index]");
          if (!parent)
            return;
          const key = Number(parent.getAttribute("data-entry-index"));
          const entry = logEntries[key];
          const isTruncated = entry.msg.length > MESSAGE_TRUNCATE_LENGTH;
          if (!isTruncated)
            return;
          showModal({
            title: "Log Message",
            content: formatLogMsg(entry.msg),
            footer: []
          });
        });
      });
    }
  };
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
      renderLogs();
      pendingLogs = [];
      debounce = null;
    }, 100);
  };
  const clearLogs = () => {
    logEntries.length = 0;
    logIds.clear();
    renderLogs();
  };
  let currentStream = null;
  let searchToken = 0;
  const beginSearch = () => {
    const token = ++searchToken;
    searchButton.disabled = true;
    searchButton.setAttribute("aria-busy", "true");
    searchButton.style.display = "none";
    stopButton.disabled = false;
    stopButton.style.display = "inline-flex";
    segmentStatus = "";
    statsStatus = "";
    updateProgressIndicator();
    return token;
  };
  const finishSearch = (token, force = false) => {
    if (!force && token !== searchToken)
      return;
    searchButton.disabled = false;
    searchButton.setAttribute("aria-busy", "false");
    searchButton.style.display = "inline-flex";
    stopButton.disabled = true;
    stopButton.style.display = "none";
    loadingSpinner.style.display = "none";
    segmentStatus = "";
    statsStatus = "";
  };
  const stopSearch = () => {
    if (!currentStream)
      return;
    const token = searchToken;
    searchToken++;
    currentStream();
    currentStream = null;
    finishSearch(token, true);
    setLoadingIndicator("Search stopped", false);
  };
  const sentinelVisible = () => {
    const rect = scrollSentinel.getBoundingClientRect();
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
        clearLogs();
        setLoadingIndicator(error, false, "red");
        return;
      }
    }
    lastQuery = query;
    if (query)
      setQueryParam("query", query);
    else
      removeQueryParam("query");
    let endDate;
    if (logEntries.length > 0)
      endDate = logEntries[logEntries.length - 1].timestamp;
    if (lastEndDate !== null && endDate === lastEndDate && !clear) {
      return;
    }
    lastEndDate = endDate || null;
    if (clear)
      clearLogs();
    if (histogramCheckbox.checked) {
      stopHistogram();
      startHistogram();
    }
    if (currentStream)
      currentStream();
    const token = beginSearch();
    streamRowsCount = 0;
    currentStream = args.streamLogs({ query, count: 200, endDate }, (log) => {
      if (token !== searchToken)
        return;
      streamRowsCount++;
      addLogs(log);
    }, (progress) => {
      if (token !== searchToken)
        return;
      if (progress.type === "segment")
        segmentStatus = describeSegmentProgress(progress);
      else
        statsStatus = describeSearchProgress(progress);
      updateProgressIndicator();
    }, () => {
      if (token !== searchToken)
        return;
      currentStream = null;
      if (streamRowsCount === 0) {
        if (logEntries.length === 0)
          setLoadingIndicator("No logs found", false);
        else
          setLoadingIndicator("No more logs", false);
      } else {
        setLoadingIndicator("", false);
      }
      finishSearch(token);
      if (streamRowsCount > 0 && sentinelVisible()) {
        queryLogs();
      }
    });
  };
  searchTextarea.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      queryLogs(true);
    }
  });
  searchButton.addEventListener("click", () => queryLogs(true));
  stopButton.addEventListener("click", stopSearch);
  downloadButton.addEventListener("click", openDownloadModal);
  const observer = new IntersectionObserver((entries) => {
    if (!entries[0].isIntersecting)
      return;
    queryLogs();
  }, { threshold: OBSERVER_THRESHOLD });
  observer.observe(scrollSentinel);
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
    streamLogs: (args, onNewLog, _onProgress, onEnd) => {
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
    streamLogs: (args, onNewLog, onProgress, onEnd) => {
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
      eventSource.addEventListener("progress", (event) => {
        const message = event;
        const raw = JSON.parse(message.data);
        if (isSegmentProgressEvent(raw) || isSearchProgressEvent(raw)) {
          onProgress(raw);
        }
      });
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
  root.root.innerHTML = "";
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
  const navbar = new Navbar({ right: [metadataCollapsible] });
  root.add(navbar);
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
    { href: "/queries", text: "Saved Queries" },
    { href: "/server", text: "Server" }
  ]);
  root.add(linkList);
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

// ts/server.ts
var serverPage = async (root) => {
  root.root.innerHTML = "";
  let info = null;
  try {
    info = await fetch("/api/v1/server_info").then((r) => r.json());
  } catch (e) {
    root.add(new KeyValueTable([{ key: "Error", value: String(e) }]));
    return;
  }
  if (!info) {
    root.add(new KeyValueTable([{ key: "Error", value: "No data" }]));
    return;
  }
  root.add(new KeyValueTable([
    { key: "Total space", value: formatBytes(info.totalBytes) },
    {
      key: "Used space",
      value: `${formatBytes(info.usedBytes)} (${info.usedPercent.toFixed(1)}%)`
    },
    { key: "Free space", value: formatBytes(info.freeBytes) },
    { key: "Upload files", value: info.uploadFilesCount.toString() },
    { key: "Upload bytes", value: formatBytes(info.uploadBytes) }
  ]));
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
    "/server": () => serverPage(container),
    "/devices": () => devicesPage(body),
    "/device/:deviceId": (params) => devicePage(body, params.deviceId),
    "/segments": () => segmentsPage(container),
    "/queries": () => queriesPage(body),
    "/segment/:segmentId": (params) => segmentPage(body, params.segmentId),
    "/pivot": () => PivotPage(body),
    "/*": () => mainPage(body)
  });
};
