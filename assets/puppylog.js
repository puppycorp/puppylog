// ts/virtual-table.ts
class VirtualTable {
  root;
  container;
  table;
  rowHeight;
  rowCount;
  bufferSize = 10;
  drawRow;
  constructor(args) {
    this.drawRow = args.drawRow;
    this.rowHeight = args.rowHeight;
    this.rowCount = args.rowCount;
    this.root = document.createElement("div");
    this.root.style.height = "500px";
    this.root.style.width = "800px";
    this.root.style.overflow = "scroll";
    this.container = document.createElement("div");
    this.container.style.position = "relative";
    this.root.appendChild(this.container);
    this.container.style.height = `${args.rowHeight * args.rowCount}px`;
    this.container.style.width = "100%";
    this.container.style.border = "1px solid black";
    this.container.innerHTML = "Virtual Table";
    this.table = document.createElement("table");
    this.container.appendChild(this.table);
    this.root.addEventListener("scroll", (e) => {
      this.onScroll(e);
    });
    this.updateVisibleRows();
  }
  onScroll(e) {
    requestAnimationFrame(() => this.updateVisibleRows());
  }
  updateVisibleRows() {
    const scrollTop = this.root.scrollTop;
    const containerHeight = this.root.clientHeight;
    console.log("containerHeight", containerHeight);
    console.log("o", this.root.scrollHeight);
    const startIndex = Math.max(0, Math.floor(scrollTop / this.rowHeight) - this.bufferSize);
    const endIndex = Math.min(this.rowCount, Math.ceil((scrollTop + containerHeight) / this.rowHeight) + this.bufferSize);
    console.log("Visible range", startIndex, endIndex);
    const content = this.drawRow(startIndex, endIndex);
    content.style.position = "absolute";
    content.style.top = `${startIndex * this.rowHeight}px`;
    this.container.innerHTML = "";
    this.container.appendChild(content);
  }
  setRowCount(rowCount) {
    this.rowCount = rowCount;
    this.container.style.height = `${this.rowHeight * rowCount + this.rowHeight * 3}px`;
    this.updateVisibleRows();
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
  rows = [];
  constructor() {
    this.header = document.createElement("tr");
    this.header.innerHTML = `<th>Timestamp</th><th>Level</th><th>message</th>`;
    this.table.appendChild(this.header);
    this.body = document.createElement("tbody");
    this.table.appendChild(this.body);
    const virtual = new VirtualTable({
      rowCount: 0,
      rowHeight: 35,
      drawRow: (start, end) => {
        let body = "";
        for (let i = start;i < end; i++) {
          const r = this.rows[i];
          body += `
                    <tr style="height: 35px">
                        <td>${r.timestamp}</td>
                        <td style="color: ${logColors[r.level]}">${r.level}</td>
                        <td>${i} - ${r.msg}</td>
                    </tr>
                    `;
        }
        this.body.innerHTML = body;
        return this.table;
      }
    });
    this.root = virtual.root;
    this.logSearcher = new LogSearcher({
      onNewLoglines: (rows) => {
        this.rows.push(...rows);
        this.rows.sort((a, b) => {
          if (this.sortDir === "asc") {
            return a.timestamp.localeCompare(b.timestamp);
          } else {
            return b.timestamp.localeCompare(a.timestamp);
          }
        });
        virtual.setRowCount(this.rows.length);
      },
      onClear: () => {
        this.body.innerHTML;
      }
    });
    this.root = virtual.root;
    this.logSearcher.search({
      count: 1e5
    });
    this.logSearcher.stream();
    window.addEventListener("scroll", (e) => {
      console.log("scroll", e);
    });
  }
  sort(dir) {
    this.sortDir = dir;
  }
}

class LogSearch {
  root;
  input;
  button;
  startDate;
  endDate;
  constructor() {
    this.root = document.createElement("div");
    this.input = document.createElement("input");
    this.input.type = "text";
    this.button = document.createElement("button");
    this.button.innerHTML = "Search";
    this.root.appendChild(this.input);
    this.root.appendChild(this.button);
    this.startDate = document.createElement("input");
    this.startDate.type = "date";
    this.root.appendChild(this.startDate);
    this.endDate = document.createElement("input");
    this.endDate.type = "date";
    this.root.appendChild(this.endDate);
  }
  getQuery() {
    return this.input.value;
  }
}

class LogSearcher {
  logEventSource;
  onClear;
  onNewLoglines;
  constructor(args) {
    this.onClear = args.onClear;
    this.onNewLoglines = args.onNewLoglines;
  }
  stream() {
    this.createEventSource("http://localhost:3000/api/logs/stream");
  }
  search(args) {
    const query = new URLSearchParams;
    if (args.startDate) {
      query.append("startDate", args.startDate);
    }
    if (args.endDate) {
      query.append("endDate", args.endDate);
    }
    if (args.search) {
      for (const s of args.search) {
        query.append("search", s);
      }
    }
    if (args.count) {
      query.append("count", args.count.toString());
    }
    const url = new URL("http://localhost:3000/api/logs");
    url.search = query.toString();
    fetch(url.toString()).then((res) => res.json()).then((data) => {
      this.onNewLoglines(data);
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
      this.onNewLoglines([JSON.parse(e.data)]);
    };
    this.logEventSource.onerror = (err) => {
      console.error("error", err);
      this.logEventSource?.close();
    };
  }
}

// ts/app.ts
window.onload = () => {
  const body = document.querySelector("body");
  if (!body) {
    throw new Error("No body element found");
  }
  const logSearch = new LogSearch;
  const t = new Logtable;
  body.appendChild(logSearch.root);
  body.appendChild(t.root);
};
