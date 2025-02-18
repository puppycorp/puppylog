// ts/devices.ts
var devicesPage = async (root) => {
  root.innerHTML = `
		<div class="page-header">
			<h1>Devices</h1>
		</div>
		<div class="logs-list" id="devicesList">
		<div class="logs-loading-indicator">Loading devices...</div>
		</div>
	`;
  try {
    const res = await fetch("/api/v1/devices").then((res2) => res2.json());
    const devicesList = document.getElementById("devicesList");
    if (!devicesList)
      return;
    devicesList.innerHTML = "";
    if (Array.isArray(res) && res.length > 0) {
      res.forEach((device) => {
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

// ts/logs.ts
var MAX_LOG_ENTRIES = 1e4;
var MESSAGE_TRUNCATE_LENGTH = 700;
var FETCH_DEBOUNCE_MS = 500;
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
  let shouldStream = getQueryParam("stream") === "true";
  const streamButton = document.createElement("button");
  const setStreamButtonText = () => {
    if (shouldStream)
      streamButton.innerHTML = "stop";
    else
      streamButton.innerHTML = "stream";
  };
  setStreamButtonText();
  optionsRightPanel.append(settingsButton, searchButton, streamButton);
  const logsList = document.createElement("div");
  logsList.className = "logs-list";
  args.root.appendChild(logsList);
  const loadingIndicator = document.createElement("div");
  args.root.appendChild(loadingIndicator);
  const addLogs = (logs) => {
    const newEntries = logs.filter((entry) => !logIds.has(entry.id));
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
					${formatTimestamp(entry.timestamp)} 
					<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
					${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
				</div>
				<div class="logs-list-row-msg" title="${entry.msg}">
					<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
					<div class="msg-full">${escapeHTML(entry.msg)}</div>
				</div>
			</div>
		`).join("");
  };
  let currentStream = null;
  const startStream = (query) => {
    const buffer = [];
    let debounce;
    currentStream = {
      query,
      close: args.streamLogs(query, (log) => {
        buffer.push(log);
        if (debounce)
          return;
        debounce = setTimeout(() => {
          addLogs(buffer);
          debounce = null;
        }, 30);
      }, () => {
        currentStream = null;
        if (!shouldStream)
          return;
        setTimeout(() => startStream(query), 1000);
      })
    };
  };
  streamButton.onclick = () => {
    shouldStream = !shouldStream;
    setStreamButtonText();
    if (shouldStream)
      setQueryParam("stream", "true");
    else
      removeQueryParam("stream");
    if (shouldStream && !currentStream)
      startStream(searchTextarea.value);
    if (!shouldStream)
      currentStream?.close();
  };
  const clearLogs = () => {
    logEntries.length = 0;
    logIds.clear();
    logsList.innerHTML = "";
  };
  const queryLogs = async (clear) => {
    const query = searchTextarea.value;
    if (currentStream?.query !== query)
      currentStream?.close();
    if (!currentStream && shouldStream)
      startStream(query);
    loadingIndicator.textContent = "Loading...";
    let endDate;
    if (logEntries.length > 0)
      endDate = logEntries[logEntries.length - 1].timestamp;
    if (clear)
      clearLogs();
    try {
      const logs = await args.fetchMore({
        count: 100,
        query,
        endDate
      });
      if (logs.length === 0) {
        loadingIndicator.textContent = "No more rows";
        moreRows = false;
        return;
      }
      moreRows = true;
      setTimeout(() => {
        moreRows = true;
      }, FETCH_DEBOUNCE_MS);
      addLogs(logs);
      loadingIndicator.textContent = "";
    } catch (err) {
      loadingIndicator.textContent = err.message;
      clearLogs();
    }
  };
  searchTextarea.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      queryLogs(true);
    }
  });
  searchButton.addEventListener("click", () => queryLogs(true));
  const observer = new IntersectionObserver((entries) => {
    if (!moreRows || !entries[0].isIntersecting)
      return;
    moreRows = false;
    queryLogs();
  }, {
    threshold: OBSERVER_THRESHOLD
  });
  observer.observe(loadingIndicator);
  let activeTimeout;
  window.onmousemove = () => {
    clearTimeout(activeTimeout);
    activeTimeout = setTimeout(() => {
      if (currentStream)
        currentStream.close();
      shouldStream = false;
      setStreamButtonText();
      removeQueryParam("stream");
    }, 300000);
  };
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
    streamLogs: (query2, onNewLog, onEnd) => {
      const streamQuery = new URLSearchParams;
      if (query2)
        streamQuery.append("query", query2);
      const streamUrl = new URL("/api/logs/stream", window.location.origin);
      streamUrl.search = streamQuery.toString();
      const eventSource = new EventSource(streamUrl);
      eventSource.onmessage = (event) => {
        const data = JSON.parse(event.data);
        onNewLog(data);
      };
      eventSource.onerror = (event) => {
        eventSource.close();
        onEnd();
      };
      return () => eventSource.close();
    },
    fetchMore: async (args) => {
      query = args.query;
      if (query)
        setQueryParam("query", query);
      else
        removeQueryParam("query");
      const urlQuery = new URLSearchParams;
      const offsetInMinutes = new Date().getTimezoneOffset();
      const offsetInHours = -offsetInMinutes / 60;
      urlQuery.append("timezone", offsetInHours.toString());
      if (args.query)
        urlQuery.append("query", args.query);
      if (args.count)
        urlQuery.append("count", args.count.toString());
      if (args.endDate)
        urlQuery.append("endDate", args.endDate);
      const url = new URL("/api/logs", window.location.origin);
      url.search = urlQuery.toString();
      const res = await fetch(url.toString());
      if (res.status === 400) {
        const err = await res.json();
        console.error("res.error", err);
        throw new Error(err.error);
      } else if (res.status !== 200) {
        const text = await res.text();
        throw new Error(text);
      }
      return res.json();
    }
  });
  return root;
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
    "/*": () => mainPage(body)
  });
};
