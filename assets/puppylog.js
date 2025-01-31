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
var routes = (routes2, container) => {
  const matcher = patternMatcher(routes2);
  const handleRoute = (path) => {
    const result = matcher.match(path);
    console.log("match result", result);
    container.innerHTML = "";
    if (!result) {
      const notFound = document.createElement("div");
      notFound.innerHTML = "Not found";
      container.appendChild(notFound);
      return notFound;
    }
    container.appendChild(result.result);
  };
  handleRoute(window.location.pathname);
  window.addEventListener("popstate", () => {
    handleRoute(window.location.pathname);
  });
  return (path) => {
    window.history.pushState({}, "", path);
    handleRoute(path);
  };
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
var logColors = {
  Debug: "blue",
  Info: "green",
  Warn: "orange",
  Error: "red"
};
var formatTimestamp = (ts) => {
  const date = new Date(ts);
  return date.toLocaleString();
};
var MAX_LOG_ENTRIES = 1e4;
var logsSearchPage = (args) => {
  const root = document.createElement("div");
  const logids = new Set;
  const logEntries = [];
  const options = document.createElement("div");
  options.style.position = "sticky";
  options.style.top = "0";
  options.style.gap = "10px";
  options.style.backgroundColor = "white";
  options.style.height = "100px";
  options.style.display = "flex";
  const searchBar = document.createElement("textarea");
  const tbody = document.createElement("tbody");
  tbody.style.width = "400px";
  const queryLogs = (query) => {
    logEntries.length = 0;
    tbody.innerHTML = "";
    last.innerHTML = "Loading...";
    args.fetchMore({
      offset: 0,
      count: 100,
      query
    });
  };
  searchBar.style.height = "100px";
  searchBar.style.resize = "none";
  searchBar.style.flexGrow = "1";
  searchBar.value = getQueryParam("query") || "";
  searchBar.onkeydown = (e) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      queryLogs(searchBar.value);
    }
  };
  options.appendChild(searchBar);
  const searchButton = document.createElement("button");
  searchButton.onclick = () => {
    queryLogs(searchBar.value);
  };
  searchButton.innerHTML = "Search";
  options.appendChild(searchButton);
  const streamButton = document.createElement("button");
  const streamButtonState = (state) => state ? "Stop<br />Stream" : "Start<br />Stream";
  streamButton.innerHTML = streamButtonState(args.isStreaming);
  streamButton.onclick = () => {
    const isStreaming = args.toggleIsStreaming();
    streamButton.innerHTML = streamButtonState(isStreaming);
  };
  options.appendChild(streamButton);
  root.appendChild(options);
  const table = document.createElement("table");
  table.style.width = "100%";
  const thead = document.createElement("thead");
  thead.style.position = "sticky";
  thead.style.top = "100px";
  thead.style.backgroundColor = "white";
  thead.innerHTML = `
\t\t<tr>
\t\t\t<th>Timestamp</th>
\t\t\t<th>Level</th>
\t\t\t<th>Props</th>
\t\t\t<th>Message</th>
\t\t</tr>
\t`;
  table.appendChild(thead);
  table.appendChild(tbody);
  const tableWrapper = document.createElement("div");
  tableWrapper.style.overflow = "auto";
  tableWrapper.appendChild(table);
  root.appendChild(table);
  const last = document.createElement("div");
  last.style.height = "100px";
  last.innerHTML = "Loading...";
  root.appendChild(last);
  let moreRows = true;
  const observer = new IntersectionObserver(() => {
    console.log("intersect");
    if (!moreRows)
      return;
    console.log("need to fetch more");
    moreRows = false;
    args.fetchMore({
      offset: logEntries.length,
      count: 100,
      query: searchBar.value
    });
  }, {
    root: null,
    rootMargin: "0px",
    threshold: 0.1
  });
  observer.observe(last);
  const escapeHTML = (str) => {
    const div = document.createElement("div");
    div.textContent = str;
    return div.innerHTML;
  };
  return {
    root,
    setIsStreaming: (isStreaming) => {
      streamButton.innerHTML = streamButtonState(isStreaming);
    },
    onError(err) {
      last.innerHTML = err;
    },
    addLogEntries: (entries) => {
      if (entries.length === 0) {
        last.innerHTML = "No more rows";
        return;
      }
      setTimeout(() => {
        moreRows = true;
      }, 500);
      for (const entry of entries) {
        if (logids.has(entry.id)) {
          continue;
        }
        logids.add(entry.id);
        logEntries.push(entry);
      }
      logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
      if (logEntries.length > MAX_LOG_ENTRIES) {
        if (tableWrapper.scrollTop === 0) {
          const removed = logEntries.splice(0, MAX_LOG_ENTRIES);
          for (const r of removed) {
            logids.delete(r.id);
          }
        }
      }
      const body = `
\t\t\t\t${logEntries.map((r) => {
        return `
\t\t\t\t\t\t<tr style="height: 35px">
\t\t\t\t\t\t\t<td style="white-space: nowrap; vertical-align: top; text-align: center;">${formatTimestamp(r.timestamp)}</td>
\t\t\t\t\t\t\t<td style="color: ${logColors[r.level]}; vertical-align: top; text-align: center;text-align: center;">${r.level}</td>
\t\t\t\t\t\t\t<td style="vertical-align: top; text-align: left;">${r.props.map((p) => `${p.key}=${p.value}`).join("<br />")}</td>
\t\t\t\t\t\t\t<td style="word-break: break-all; vertical-align: top">${escapeHTML(`${r.msg.slice(0, 700)}${r.msg.length > 700 ? "..." : ""}`)}</td>
\t\t\t\t\t\t</tr>
\t\t\t\t\t`;
      }).join("")}
\t\t\t`;
      tbody.innerHTML = body;
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
var logtableTest = () => {
  const { root, addLogEntries } = logsSearchPage({
    isStreaming: false,
    toggleIsStreaming: () => false,
    fetchMore: (args) => {
      const logEntries = [];
      for (let i = args.offset;i < args.offset + args.count; i++) {
        logEntries.push({
          timestamp: new Date().toISOString(),
          level: "Info",
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

// ts/main-page.ts
var mainPage = () => {
  let query = getQueryParam("query") || "";
  let logEventSource = null;
  let isStreaming = getQueryParam("stream") === "true";
  let lastStreamQuery = null;
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
      addLogEntries([data]);
    };
    logEventSource.onerror = (event) => {
      console.error("EventSource error", event);
      if (logEventSource)
        logEventSource.close();
      setIsStreaming(false);
    };
  };
  const { root, addLogEntries, onError, setIsStreaming } = logsSearchPage({
    isStreaming,
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

// ts/app.ts
window.onload = () => {
  const body = document.querySelector("body");
  if (!body) {
    throw new Error("No body element found");
  }
  const navigate = routes({
    "/tests": () => {
      const tests = document.createElement("div");
      tests.innerHTML = "Tests";
      return tests;
    },
    "/tests/logtable": () => logtableTest(),
    "/*": () => mainPage()
  }, body);
};
