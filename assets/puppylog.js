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
  logsOptions.className = "logs-options";
  args.root.appendChild(logsOptions);
  const searchTextarea = document.createElement("textarea");
  searchTextarea.className = "logs-search-bar";
  searchTextarea.placeholder = "Search logs (ctrl+enter to search)";
  searchTextarea.value = args.query || "";
  logsOptions.appendChild(searchTextarea);
  const optionsRightPanel = document.createElement("div");
  optionsRightPanel.className = "logs-options-right-panel";
  logsOptions.appendChild(optionsRightPanel);
  const settingsButton = document.createElement("button");
  settingsButton.innerHTML = settingsSvg;
  settingsButton.onclick = () => navigate("/settings");
  const searchButton = document.createElement("button");
  searchButton.innerHTML = searchSvg;
  const streamButton = document.createElement("button");
  streamButton.innerHTML = "stream";
  optionsRightPanel.append(settingsButton, searchButton, streamButton);
  const logsList = document.createElement("div");
  logsList.className = "logs-list";
  args.root.appendChild(logsList);
  const loadingIndicator = document.createElement("div");
  args.root.appendChild(loadingIndicator);
  const queryLogs = (query) => {
    logEntries.length = 0;
    logIds.clear();
    logsList.innerHTML = "";
    loadingIndicator.textContent = "Loading...";
    args.fetchMore({ offset: 0, count: 100, query });
  };
  searchTextarea.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      queryLogs(searchTextarea.value);
    }
  });
  searchButton.addEventListener("click", () => queryLogs(searchTextarea.value));
  const updateStreamButtonText = (isStreaming) => {
    streamButton.innerHTML = isStreaming ? "stop" : "stream";
  };
  streamButton.addEventListener("click", () => {
    const isStreaming = args.toggleIsStreaming();
    updateStreamButtonText(isStreaming);
  });
  const observer = new IntersectionObserver((entries) => {
    if (!moreRows || !entries[0].isIntersecting)
      return;
    moreRows = false;
    args.fetchMore({
      offset: logEntries.length,
      count: 100,
      query: searchTextarea.value
    });
  }, {
    threshold: OBSERVER_THRESHOLD
  });
  observer.observe(loadingIndicator);
  return {
    setIsStreaming: updateStreamButtonText,
    onError(err) {
      loadingIndicator.textContent = err;
    },
    addLogEntries(entries) {
      if (entries.length === 0) {
        loadingIndicator.textContent = "No more rows";
        return;
      }
      setTimeout(() => {
        moreRows = true;
      }, FETCH_DEBOUNCE_MS);
      const newEntries = entries.filter((entry) => !logIds.has(entry.id));
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
\t\t\t\t<div class="logs-list-row">
\t\t\t\t\t<div>
\t\t\t\t\t\t${formatTimestamp(entry.timestamp)} 
\t\t\t\t\t\t<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
\t\t\t\t\t\t${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
\t\t\t\t\t</div>
\t\t\t\t\t<div class="logs-list-row-msg">
\t\t\t\t\t\t${escapeHTML(truncateMessage(entry.msg))}
\t\t\t\t\t</div>
\t\t\t\t</div>
\t\t\t`).join("");
    }
  };
};

// ts/logtable-test.ts
var logline = (length, linebreaks) => {
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
};
var randomLogline = () => {
  const length = Math.floor(Math.random() * 100);
  const linebreaks = Math.floor(Math.random() * 10);
  return logline(length, linebreaks);
};
var logtableTest = (root) => {
  const { addLogEntries } = logsSearchPage({
    root,
    isStreaming: false,
    toggleIsStreaming: () => false,
    fetchMore: (args) => {
      const logEntries = [];
      for (let i = args.offset;i < args.offset + args.count; i++) {
        logEntries.push({
          id: i.toString(),
          timestamp: new Date().toISOString(),
          level: "info",
          props: [
            { key: "key", value: "value" },
            { key: "key2", value: "value2" }
          ],
          msg: `[${i}] ${randomLogline()}`
        });
      }
      addLogEntries(logEntries);
    }
  });
  return root;
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

// ts/main-page.ts
var mainPage = (root) => {
  let query = getQueryParam("query") || "";
  let logEventSource = null;
  let isStreaming = getQueryParam("stream") === "true";
  let lastStreamQuery = null;
  let logEntriesBuffer = [];
  let timeout = null;
  const startStream = (query2) => {
    if (lastStreamQuery === query2)
      return;
    lastStreamQuery = query2;
    if (logEventSource)
      logEventSource.close();
    logEventSource = null;
    const streamQuery = new URLSearchParams;
    if (query2)
      streamQuery.append("query", query2);
    const streamUrl = new URL("/api/logs/stream", window.location.origin);
    streamUrl.search = streamQuery.toString();
    logEventSource = new EventSource(streamUrl);
    logEventSource.onopen = () => setIsStreaming(true);
    logEventSource.onmessage = (event) => {
      const data = JSON.parse(event.data);
      logEntriesBuffer.push(data);
      if (timeout)
        return;
      timeout = setTimeout(() => {
        addLogEntries(logEntriesBuffer);
        logEntriesBuffer = [];
        timeout = null;
      }, 30);
    };
    logEventSource.onerror = (event) => {
      console.error("EventSource error", event);
      if (logEventSource)
        logEventSource.close();
      setIsStreaming(false);
    };
  };
  const { addLogEntries, onError, setIsStreaming } = logsSearchPage({
    root,
    isStreaming,
    query,
    toggleIsStreaming: () => {
      isStreaming = !isStreaming;
      if (isStreaming) {
        startStream(query);
        setQueryParam("stream", "true");
      } else {
        if (logEventSource)
          logEventSource.close();
        lastStreamQuery = null;
        removeQueryParam("stream");
      }
      return isStreaming;
    },
    fetchMore: (args) => {
      query = args.query;
      if (query)
        setQueryParam("query", query);
      else
        removeQueryParam("query");
      console.log("fetchMore", args);
      const urlQuery = new URLSearchParams;
      const offsetInMinutes = new Date().getTimezoneOffset();
      const offsetInHours = -offsetInMinutes / 60;
      urlQuery.append("timezone", offsetInHours.toString());
      if (args.query)
        urlQuery.append("query", args.query);
      urlQuery.append("count", args.count.toString());
      urlQuery.append("offset", args.offset.toString());
      const url = new URL("/api/logs", window.location.origin);
      url.search = urlQuery.toString();
      fetch(url.toString()).then(async (res) => {
        if (res.status === 400) {
          const err = await res.json();
          console.error("res.error", err);
          onError(err.error);
          console.log("res", res);
          throw new Error("Failed to fetch logs");
        }
        return res.json();
      }).then((data) => {
        addLogEntries(data);
      }).catch((err) => {
        console.error("error", err);
      });
      if (isStreaming)
        startStream(query);
      else {
        if (logEventSource) {
          logEventSource.close();
        }
      }
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
  root.appendChild(infoText);
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
  root.appendChild(saveButton);
  root.appendChild(textarea);
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
    "/*": () => mainPage(body)
  });
};
