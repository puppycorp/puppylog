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

// ts/devices.ts
var saveDeviceSettings = async (device) => {
  await fetch(`/api/v1/device/${device.id}/settings`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      sendLogs: device.sendLogs,
      filterLevel: device.filterLevel
    })
  });
};
var levels = ["trace", "debug", "info", "warn", "error", "fatal"];
var createDeviceRow = (device) => {
  const deviceRow = document.createElement("div");
  deviceRow.classList.add("list-row");
  const idCell = document.createElement("div");
  idCell.className = "table-cell";
  idCell.innerHTML = `<strong>ID:</strong> ${device.id}`;
  deviceRow.appendChild(idCell);
  const createdAtCell = document.createElement("div");
  createdAtCell.className = "table-cell";
  createdAtCell.innerHTML = `<strong>Created at:</strong> ${new Date(device.createdAt).toLocaleString()}`;
  deviceRow.appendChild(createdAtCell);
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
  deviceRow.appendChild(filterLevelCell);
  const lastUploadCell = document.createElement("div");
  lastUploadCell.className = "table-cell";
  lastUploadCell.innerHTML = `<strong>Last upload:</strong> ${new Date(device.lastUploadAt).toLocaleString()}`;
  deviceRow.appendChild(lastUploadCell);
  const logsCountCell = document.createElement("div");
  logsCountCell.className = "table-cell";
  logsCountCell.innerHTML = `<strong>Logs count:</strong> ${formatNumber(device.logsCount)}`;
  deviceRow.appendChild(logsCountCell);
  const logsSizeCell = document.createElement("div");
  logsSizeCell.className = "table-cell";
  logsSizeCell.innerHTML = `<strong>Logs size:</strong> ${formatBytes(device.logsSize)} bytes`;
  deviceRow.appendChild(logsSizeCell);
  const averageLogSizeCell = document.createElement("div");
  averageLogSizeCell.className = "table-cell";
  averageLogSizeCell.innerHTML = `<strong>Average log size:</strong> ${formatBytes(device.logsSize / device.logsCount)}`;
  deviceRow.appendChild(averageLogSizeCell);
  const logsPerSecondCell = document.createElement("div");
  logsPerSecondCell.className = "table-cell";
  const lastUploadDate = new Date(device.lastUploadAt);
  const createdAtDate = new Date(device.createdAt);
  const diff = lastUploadDate.getTime() - createdAtDate.getTime();
  const seconds = diff / 1000;
  const logsPerSecond = device.logsCount / seconds;
  logsPerSecondCell.innerHTML = `<strong>Logs per second:</strong> ${logsPerSecond.toFixed(2)}`;
  deviceRow.appendChild(logsPerSecondCell);
  const sendLogsCell = document.createElement("div");
  sendLogsCell.className = "table-cell";
  sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`;
  deviceRow.appendChild(sendLogsCell);
  const deviceSaveButton = document.createElement("button");
  deviceSaveButton.textContent = "Save";
  deviceSaveButton.style.visibility = "hidden";
  deviceRow.appendChild(deviceSaveButton);
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
  return deviceRow;
};
var devicesPage = async (root) => {
  root.innerHTML = `
\t\t<div class="page-header">
\t\t\t<h1 style="flex-grow: 1">Devices</h1>
\t\t\t<div id="devicesSummary">Loading summary...</div>
\t\t</div>
\t\t
\t\t<div id="devicesList">
\t\t\t<div class="logs-loading-indicator">Loading devices...</div>
\t\t</div>

\t`;
  try {
    const res = await fetch("/api/v1/devices");
    const devices = await res.json();
    const summaryEl = document.getElementById("devicesSummary");
    if (summaryEl) {
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
      const totalSeconds = (latestTimestamp - earliestTimestamp) / 1000;
      const averageLogSize = totalLogsCount > 0 ? totalLogsSize / totalLogsCount : 0;
      summaryEl.innerHTML = `
\t\t\t\t<div><strong>Total Logs Count:</strong> ${formatNumber(totalLogsCount)}</div>
\t\t\t\t<div><strong>Total Logs Size:</strong> ${formatBytes(totalLogsSize)}</div>
\t\t\t\t<div><strong>Average Log Size:</strong> ${formatBytes(averageLogSize)}</div>
\t\t\t\t<div><strong>Logs per Second:</strong> ${totalLogsPerSecond.toFixed(2)}</div>
\t\t\t`;
    }
    const devicesList = document.getElementById("devicesList");
    if (!devicesList)
      return;
    devicesList.innerHTML = "";
    if (Array.isArray(devices) && devices.length > 0) {
      devices.forEach((device) => {
        devicesList.appendChild(createDeviceRow(device));
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

// ts/common.ts
var showModal = (content, title) => {
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
  modalContent.style.maxWidth = "500px";
  modalContent.style.wordWrap = "break-word";
  modalContent.style.wordBreak = "break-all";
  modalContent.addEventListener("click", (e) => {
    e.stopPropagation();
  });
  const modalTitle = document.createElement("h3");
  modalTitle.textContent = title;
  modalContent.appendChild(modalTitle);
  modalContent.appendChild(content);
  const closeModalBtn = document.createElement("button");
  closeModalBtn.textContent = "Close";
  closeModalBtn.style.marginTop = "8px";
  closeModalBtn.addEventListener("click", () => {
    modalOverlay.remove();
  });
  modalContent.appendChild(closeModalBtn);
  modalOverlay.addEventListener("click", () => {
    modalOverlay.remove();
  });
  modalOverlay.appendChild(modalContent);
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
  const patternParts = pattern.split("/");
  const pathParts = path.split("/");
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
    if (patternPart === "*")
      return params;
    if (patternPart.startsWith(":")) {
      const paramName = patternPart.slice(1);
      params[paramName] = pathPart;
      continue;
    }
    if (patternPart !== pathPart)
      return null;
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
var formatTimestamp = (ts) => {
  const date = new Date(ts);
  return date.toLocaleString();
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
  const settingsButton = document.createElement("button");
  settingsButton.innerHTML = settingsSvg;
  settingsButton.onclick = () => navigate("/settings");
  const searchButton = document.createElement("button");
  searchButton.innerHTML = searchSvg;
  optionsRightPanel.append(settingsButton, searchButton);
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
\t\t\t\t<div class="list-row">
\t\t\t\t\t<div>
\t\t\t\t\t\t${formatTimestamp(entry.timestamp)} 
\t\t\t\t\t\t<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
\t\t\t\t\t\t${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
\t\t\t\t\t</div>
\t\t\t\t\t<div class="logs-list-row-msg" title="${entry.msg}">
\t\t\t\t\t\t<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
\t\t\t\t\t</div>
\t\t\t\t</div>
\t\t\t`).join("");
      document.querySelectorAll(".logs-list-row-msg").forEach((el, key) => {
        el.addEventListener("click", () => {
          console.log("click", key);
          const div = document.createElement("div");
          const entry = logEntries[key];
          div.innerHTML = escapeHTML(entry.msg);
          showModal(div, "Log Message");
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
    if (clear)
      clearLogs();
    if (currentStream)
      currentStream();
    currentStream = args.streamLogs({ query, count: 100, endDate }, (log) => {
      streamRowsCount++;
      addLogs(log);
    }, () => {
      currentStream = null;
      loadingIndicator.textContent = "";
      console.log("stream rows count", streamRowsCount);
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
function randomLogline() {
  const length = Math.floor(Math.random() * 100);
  const linebreaks = Math.floor(Math.random() * 10);
  return logline(length, linebreaks);
}
var logtableTest = (root) => {
  logsSearchPage({
    root,
    fetchMore: async (args) => {
      await new Promise((resolve) => setTimeout(resolve, 500));
      const logs = [];
      const count = args.count || 100;
      for (let i = 0;i < count; i++) {
        logs.push({
          id: `${Date.now()}-${i}`,
          timestamp: new Date(Date.now() - i * 1000).toISOString(),
          level: "info",
          props: [
            { key: "key", value: "value" },
            { key: "key2", value: "value2" }
          ],
          msg: `[${i}] ${randomLogline()}`
        });
      }
      return logs;
    },
    streamLogs: (query, onNewLog, onEnd) => {
      const intervalId = setInterval(() => {
        onNewLog({
          id: `${Date.now()}-stream`,
          timestamp: new Date().toISOString(),
          level: "debug",
          props: [{ key: "stream", value: "true" }],
          msg: `Streamed log: ${randomLogline()}`
        });
      }, 2000);
      const timeoutId = setTimeout(() => {
        clearInterval(intervalId);
        onEnd();
      }, 1e4);
      return () => {
        clearInterval(intervalId);
        clearTimeout(timeoutId);
      };
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
      const streamUrl = new URL("/api/v1/logs", window.location.origin);
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
    { logLevel: "Info", deviceId: "Device1", message: "Started process", timestamp: 1610000000000 },
    { logLevel: "Error", deviceId: "Device2", message: "Failed to load module", timestamp: 1610000001000 },
    { logLevel: "Warning", deviceId: "Device1", message: "Memory usage high", timestamp: 1610000002000 },
    { logLevel: "Info", deviceId: "Device3", message: "Process completed", timestamp: 1610000003000 },
    { logLevel: "Error", deviceId: "Device1", message: "Unhandled exception", timestamp: 1610000004000 },
    { logLevel: "Debug", deviceId: "Device2", message: "Debugging info", timestamp: 1610000005000 }
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
    showModal(hello);
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
var segmentsPage = async (root) => {
  const res = await fetch("/api/v1/segments").then((res2) => res2.json());
  const totalSegments = res.length;
  const totalOriginalSize = res.reduce((sum, seg) => sum + seg.originalSize, 0);
  const totalCompressedSize = res.reduce((sum, seg) => sum + seg.compressedSize, 0);
  const totalLogsCount = res.reduce((sum, seg) => sum + seg.logsCount, 0);
  const compressRatio = totalCompressedSize / totalOriginalSize * 100;
  root.innerHTML = `
\t\t<div class="page-header">
\t\t\t<h1 style="flex-grow: 1">Segments</h1>
\t\t\t<div class="summary">
\t\t\t\t<div><strong>Total segments:</strong> ${formatNumber(totalSegments)}</div>
\t\t\t\t<div><strong>Total original size:</strong> ${formatBytes(totalOriginalSize)}</div>
\t\t\t\t<div><strong>Total compressed size:</strong> ${formatBytes(totalCompressedSize)}</div>
\t\t\t\t<div><strong>Total logs count:</strong> ${formatNumber(totalLogsCount)}</div>
\t\t\t\t<div><strong>Compression ratio:</strong> ${compressRatio.toFixed(2)}%</div>
\t\t\t</div>
\t\t</div>
\t\t<div>
\t\t\t${res.map((segment) => `
\t\t\t\t<div class="list-row">
\t\t\t\t\t<div class="table-cell"><strong>Segment ID:</strong> ${formatNumber(segment.id)}</div>
\t\t\t\t\t<div class="table-cell"><strong>First timestamp:</strong> ${segment.firstTimestamp}</div>
\t\t\t\t\t<div class="table-cell"><strong>Last timestamp:</strong> ${segment.lastTimestamp}</div>
\t\t\t\t\t<div class="table-cell"><strong>Original size:</strong> ${formatBytes(segment.originalSize)}</div>
\t\t\t\t\t<div class="table-cell"><strong>Compressed size:</strong> ${formatBytes(segment.compressedSize)}</div>
\t\t\t\t\t<div class="table-cell"><strong>Logs count:</strong> ${formatNumber(segment.logsCount)}</div>
\t\t\t\t\t<div class="table-cell"><strong>Compression ratio:</strong> ${(segment.compressedSize / segment.originalSize * 100).toFixed(2)}%</div>
\t\t\t\t</div>
\t\t\t`).join("")}
\t\t</div>
\t`;
};

// ts/settings.ts
var settingsPage = (root) => {
  const infoText = document.createElement("div");
  infoText.style.color = "red";
  let originalQuery = "";
  const updateQuery = (query) => {
    fetch("/api/settings/query", {
      method: "POST",
      body: query
    }).then((res) => {
      if (!res.ok) {
        console.error("Failed to fetch query", res);
        return;
      }
      originalQuery = query;
      infoText.innerHTML = "";
    }).catch((err) => {
      console.error("Failed to update query", err);
    });
  };
  root.innerHTML = "";
  const header = document.createElement("h1");
  header.innerHTML = "Settings";
  root.appendChild(header);
  const collectionQuery = document.createElement("h2");
  collectionQuery.innerHTML = "Collection query";
  root.appendChild(collectionQuery);
  const textarea = document.createElement("textarea");
  textarea.style.width = "100%";
  textarea.style.height = "100px";
  textarea.style.resize = "none";
  root.appendChild(textarea);
  textarea.oninput = (e) => {
    console.log("onchange", textarea.value);
    if (originalQuery === textarea.value)
      infoText.innerHTML = "";
    else
      infoText.innerHTML = "Unsaved changes";
  };
  fetch("/api/settings/query").then((res) => {
    if (!res.ok) {
      console.error("Failed to fetch query", res);
    }
    return res.text();
  }).then((query) => {
    console.log("query", query);
    originalQuery = query;
    textarea.value = query;
  }).catch((err) => {
    console.error("Failed to fetch query", err);
  });
  const saveButton = document.createElement("button");
  saveButton.innerHTML = "Save";
  saveButton.onclick = () => {
    updateQuery(textarea.value);
  };
  root.appendChild(infoText);
  root.appendChild(saveButton);
  return root;
};

// ts/app.ts
window.onload = () => {
  const body = document.querySelector("body");
  if (!body) {
    throw new Error("No body element found");
  }
  routes({
    "/tests/logs": () => logtableTest(body),
    "/settings": () => settingsPage(body),
    "/devices": () => devicesPage(body),
    "/segments": () => segmentsPage(body),
    "/pivot": () => PivotPage(body),
    "/*": () => mainPage(body)
  });
};
