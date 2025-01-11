// ts/logs.ts
var logColors = {
  Debug: "blue",
  Info: "green",
  Warn: "orange",
  Error: "red"
};

class Logtable {
  root;
  header;
  body;
  sortDir = "desc";
  logSearcher;
  constructor() {
    this.root = document.createElement("table");
    this.header = document.createElement("tr");
    this.header.innerHTML = `<th>Timestamp</th><th>Level</th><th>message</th>`;
    this.root.appendChild(this.header);
    this.body = document.createElement("tbody");
    this.root.appendChild(this.body);
    this.logSearcher = new LogSearcher({
      onNewLoglines: (rows) => {
        this.addRows(rows);
      },
      onClear: () => {
        this.body.innerHTML;
      }
    });
    this.logSearcher.search({});
  }
  addRows(rows) {
    console.log("Adding rows", rows);
    for (const r of rows) {
      const row = document.createElement("tr");
      row.innerHTML = `<td>${r.timestamp}</td><td style="color: ${logColors[r.level]}">${r.level}</td><td>${r.msg}</td>`;
      if (this.sortDir === "asc") {
        this.body.prepend(row);
      } else {
        this.body.appendChild(row);
      }
    }
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
    const url = new URL("http://localhost:3000/api/logs/stream");
    url.search = query.toString();
    this.createEventSource(url.toString());
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
