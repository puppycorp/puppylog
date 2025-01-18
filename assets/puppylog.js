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

// ts/virtual-table.ts
class VirtualTable {
  root;
  container;
  table;
  rowHeight;
  rowCount;
  bufferSize = 10;
  needMoreRows = false;
  drawRow;
  fetchMore;
  constructor(args) {
    this.drawRow = args.drawRow;
    this.fetchMore = args.fetchMore;
    this.rowHeight = args.rowHeight;
    this.rowCount = args.rowCount;
    this.root = document.createElement("div");
    this.root.style.height = "800px";
    this.root.style.width = "100%";
    this.root.style.overflow = "auto";
    this.container = document.createElement("div");
    this.container.style.position = "relative";
    this.root.appendChild(this.container);
    this.container.style.height = `${args.rowHeight * args.rowCount}px`;
    this.container.style.width = "100%";
    this.container.style.marginTop = "50px";
    this.container.style.marginBottom = "50px";
    this.container.innerHTML = "Virtual Table";
    this.table = document.createElement("table");
    this.container.appendChild(this.table);
    this.root.addEventListener("scroll", (e) => {
      this.onScroll(e);
    });
    const handleObserver = (entries) => {
      console.log("Intersection observer", entries);
    };
    const observer = new IntersectionObserver(handleObserver, {
      root: this.root,
      rootMargin: "0px",
      threshold: 0.1
    });
    setTimeout(() => {
      if (this.fetchMore)
        this.fetchMore();
    });
  }
  onScroll(e) {
    requestAnimationFrame(() => this.updateVisibleRows());
  }
  updateVisibleRows() {
    const scrollTop = this.root.scrollTop;
    const containerHeight = this.root.clientHeight;
    const startIndex = Math.max(0, Math.floor(scrollTop / this.rowHeight) - this.bufferSize);
    const endIndex = Math.min(this.rowCount, Math.ceil((scrollTop + containerHeight) / this.rowHeight) + this.bufferSize);
    const content = this.drawRow(startIndex, endIndex);
    content.style.position = "absolute";
    content.style.top = `${startIndex * this.rowHeight}px`;
    this.container.innerHTML = "";
    this.container.appendChild(content);
    const rootRect = this.root.getBoundingClientRect();
    const containerRect = this.container.getBoundingClientRect();
    const rootBottom = rootRect.bottom;
    const containerBottom = containerRect.bottom;
    requestAnimationFrame(() => {
      if (containerBottom < rootBottom + 3 * this.rowHeight) {
        console.log("need more rows");
        if (this.needMoreRows)
          return;
        this.needMoreRows = true;
        if (this.fetchMore)
          this.fetchMore();
      }
    });
  }
  setRowCount(rowCount) {
    console.log("Setting row count", rowCount);
    this.rowCount = rowCount;
    this.container.style.height = `${this.rowHeight * rowCount + this.rowHeight * 3}px`;
    this.updateVisibleRows();
    this.needMoreRows = false;
  }
}

// ts/logs.ts
var logColors = {
  Debug: "blue",
  Info: "green",
  Warn: "orange",
  Error: "red"
};

class Logtable {
  root;
  table = document.createElement("table");
  header;
  body;
  sortDir = "desc";
  logSearcher;
  virtual;
  errorText;
  constructor() {
    this.root = document.createElement("div");
    this.header = document.createElement("head");
    this.header.innerHTML = `<tr><th>Timestamp</th><th>Level</th><th>Props</th><th>Message</th></tr>`;
    this.table.appendChild(this.header);
    this.body = document.createElement("tbody");
    this.table.appendChild(this.body);
    this.logSearcher = new LogSearcher({
      onNewLoglines: this.onNewLoglines.bind(this),
      onClear: () => {
      },
      onError: (err) => {
        this.errorText.innerHTML = err;
      }
    });
    this.virtual = new VirtualTable({
      rowCount: 0,
      rowHeight: 35,
      drawRow: (start, end) => {
        let body = "";
        for (let i = start;i < end; i++) {
          const r = this.logSearcher.logEntries[i];
          body += `
                    <tr style="height: 35px">
                        <td style="white-space: nowrap">${r.timestamp}</td>
                        <td style="color: ${logColors[r.level]}">${r.level}</td>
\t\t\t\t\t\t<td>${r.props.map((p) => p.join("=")).join(", ")}</td>
                        <td style="word-break: break-all">${r.msg}</td>
                    </tr>
                    `;
        }
        this.body.innerHTML = body;
        return this.table;
      },
      fetchMore: this.fetchMore.bind(this)
    });
    const searchOptions = new LogSearchOptions({
      searcher: this.logSearcher
    });
    this.root.appendChild(searchOptions.root);
    this.errorText = document.createElement("div");
    this.errorText.style.color = "red";
    this.root.appendChild(this.errorText);
    this.root.appendChild(this.virtual.root);
    this.logSearcher.stream();
    window.addEventListener("scroll", (e) => {
      console.log("scroll", e);
    });
  }
  onNewLoglines() {
    console.log("onNewLoglines");
    this.virtual.setRowCount(this.logSearcher.logEntries.length);
  }
  fetchMore() {
    if (!this.logSearcher)
      return;
    console.log("fetchMore");
    this.logSearcher.fetchMore();
  }
  sort(dir) {
    this.sortDir = dir;
  }
}

class LogSearchOptions {
  root;
  input;
  button;
  searcher;
  constructor(args) {
    this.root = document.createElement("div");
    this.root.style.display = "flex";
    this.root.style.gap = "10px";
    this.input = document.createElement("textarea");
    this.input.value = getQueryParam("query") || "";
    this.input.rows = 4;
    this.input.style.width = "400px";
    this.input.onkeydown = (e) => {
      console.log("key: ", e.key, " shift: ", e.shiftKey);
      if (e.key === "Enter" && !e.shiftKey) {
        console.log("preventing default");
        e.preventDefault();
        this.searcher.setQuery(this.input.value);
      }
    };
    this.button = document.createElement("button");
    this.button.onclick = () => {
      this.searcher.setQuery(this.input.value);
    };
    this.button.innerHTML = "Search";
    this.root.appendChild(this.input);
    this.root.appendChild(this.button);
    this.searcher = args.searcher;
  }
  getQuery() {
    return this.input.value;
  }
}

class LogSearcher {
  logEventSource;
  sortDir = "desc";
  onClear;
  onNewLoglines;
  onError;
  logEntries = [];
  firstDate;
  lastDate;
  query = "";
  offset = 0;
  count = 100;
  alreadyFetched = false;
  constructor(args) {
    this.onClear = args.onClear;
    this.onNewLoglines = args.onNewLoglines;
    this.onError = args.onError;
    this.query = getQueryParam("query") || "";
  }
  stream() {
    this.createEventSource("http://localhost:3337/api/logs/stream");
  }
  setQuery(query) {
    this.query = query;
    this.offset = 0;
    this.alreadyFetched = false;
    setQueryParam("query", query);
    this.logEntries = [];
    this.fetchMore();
  }
  fetchMore() {
    if (this.alreadyFetched)
      return;
    this.alreadyFetched = true;
    const offsetInMinutes = new Date().getTimezoneOffset();
    const offsetInHours = -offsetInMinutes / 60;
    const urlQuery = new URLSearchParams;
    urlQuery.append("timezone", offsetInHours.toString());
    if (this.query) {
      urlQuery.append("query", this.query);
    }
    urlQuery.append("count", this.count.toString());
    urlQuery.append("offset", this.offset.toString());
    const url = new URL("http://localhost:3337/api/logs");
    url.search = urlQuery.toString();
    fetch(url.toString()).then(async (res) => {
      if (res.status === 400) {
        const err = await res.json();
        this.onError(err.error);
        console.log("res", res);
        throw new Error("Failed to fetch logs");
      }
      this.onError("");
      return res.json();
    }).then((data) => {
      this.logEntries.push(...data);
      this.handleSort();
      this.onNewLoglines();
      this.offset += this.count;
      if (data.length >= this.count) {
        this.alreadyFetched = false;
      }
    }).catch((err) => {
      console.error("error", err);
    });
  }
  createEventSource(url) {
    if (this.logEventSource) {
      this.logEventSource.close();
      this.onClear();
    }
    this.logEventSource = new EventSource(url);
    this.logEventSource.onmessage = (e) => {
      console.log("Got message", e.data);
      this.logEntries.push(JSON.parse(e.data));
      this.handleSort;
      this.onNewLoglines();
    };
    this.logEventSource.onerror = (err) => {
      console.error("error", err);
      this.logEventSource?.close();
    };
  }
  handleSort() {
    if (this.logEntries.length === 0)
      return;
    if (this.sortDir === "asc")
      this.logEntries.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
    else
      this.logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
    this.firstDate = this.logEntries[0].timestamp;
    this.lastDate = this.logEntries[this.logEntries.length - 1].timestamp;
  }
}

// ts/app.ts
window.onload = () => {
  const body = document.querySelector("body");
  if (!body) {
    throw new Error("No body element found");
  }
  const t = new Logtable;
  body.appendChild(t.root);
};
