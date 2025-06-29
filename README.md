# PuppyLog - Make Logging Great Again

PuppyLog is a log collection server where clients can submit logs and send queries using the Puppy Query Language (PQL) to retrieve logs. Server supports streaming logs and querying logs. Protocol is designed to be efficient and easy to implement in different environments like server, desktop, mobile and IOT devices.

## PQL - Puppy Query Language

**Compare Operators**

```
< // Smaller than
> // Larger than
= // Equal
!= // Not equal
>= // Larger or equal than
<= // Smaller or equal than
```

**Values**

```
<timestamp> = YYYY[-MM[-DD[THH[:mm[:ss]]]]
<timestamp-field> = timestamp.year | timestamp.month | timestamp.day | timestamp.hour | timestamp.minute | timestamp.second
<temporal-expression> = <timestamp-field> <compare-op> <number>
<value> = <string> | <number> | <timestamp>
```

**Expressions**

```
<property> exists // Check some property exists
<property> not exists // Check that some property does not exist
<expression> and <expression> // Check that both expressions eval to true
<expression> or <expression> // Check that either one of expression eval to true
// Expression inside parethesis will be evaluated first
(<expression> <bool-operator> <expression>) <boolean-operator> (<expression> <bool-operator> <expression>)
<property> like "<string>" // Property value contains <string>
<property> not like "<string>" // Property value does not contain <string>
<property> <compare-op> <value> // Compares property value with value
<property> <compare-op> "<string>" // Compares property value with string
<property> in (<value1>, <value2>, ...)
<property> not in (<value1>, <value2>, ...)
<property> matches <regex>
<property> not matches <regex>
```

To search for a string that contains a quote character, escape it with a backslash. Example:
`msg like "\"error\""`

**Type Coercion**

```
<string> -> <number> // Try top parse and fail if invalid
<number> -> <string> // Convert to string representation
<bool> -> <number> // true = 1 and false = 0
<string> -> <bool> // "true" or "false" or "1" or "0"
<bool> -> <string> // "true" or "false"
```

**Operator Precedence**

```
1. Parenthese ()
2. Comparison operators
3. AND, OR
```

## Data Structures

### Logentry

Logline is a binary structure which stores log information. Each LogEntry uniqueness is ensured by timestamp and random field so it is very unlikely to have same logentry id twice. Users could also use their custom random field could include something like device id for per device uniqueness.

| Field      | Size | Description                          |
| ---------- | ---- | ------------------------------------ |
| Version    | 2    | Version of the logentry (current 1)  |
| Timestamp  | 8    | Timestamp of the log in micros       |
| Random     | 4    | Ensure uniqueness within microsecond |
| Level      | 1    | Log level                            |
| PropsCount | 1    | Property count                       |
| Props      | x    | Properties of the logentry           |
| MsgLen     | 4    | Length of the message                |
| Message    | x    | Log message payload                  |

**Loglevel**

| Value | Description |
| ----- | ----------- |
| 1     | Trace       |
| 2     | Debug       |
| 3     | Info        |
| 4     | Warning     |
| 5     | Error       |
| 6     | Fatal       |

**Property**

| Field  | Size | Description           |
| ------ | ---- | --------------------- |
| KeyLen | 1    | Length of the key     |
| Key    | x    | Key of the property   |
| ValLen | 1    | Length of the value   |
| Value  | x    | Value of the property |

### LogBatch

Logbatch is a binary structure which stores multiple loglines. Logbatch is compressed with gzip or zstd. Logbatch is used to send multiple loglines to server in one go. Supported decryption by server is gzip and zstd.

| Field      | Size | Description                     |
| ---------- | ---- | ------------------------------- |
| Version    | 2    | Version of logbatch (current 1) |
| Seq        | 4    | Sequence number of the logbatch |
| Crc32      | 4    | CRC32 checksum of the logbatch  |
| Size       | 4    | Payload size in bytes           |
| LogEntries | x    | LogEntries in binary format     |

## API

### GET /api/logs

Search logs with PQL query. Returns logs in json format.

#### Query

| Field   | DataType | Description                                   |
| ------- | -------- | --------------------------------------------- |
| count   | int      | Number of logs to return (default 200)        |
| query   | string   | Query string in PQL format                    |
| endDate | string   | Only include logs before this timestamp (ISO) |

#### Response

```json
[
    {
		"id": "123456789",
        "timestamp": "2025-01-01T12:00:00",
        "level": "trace" | "debug" | "info" | "warning" | "error" | "fatal",
		"props": [
			{
				"key": "key",
				"value": "value"
			}
		],
        "message": "Log message"
    }
]
```

### GET /api/logs/stream

#### Query

| Field   | DataType | Description                                |
| ------- | -------- | ------------------------------------------ |
| query   | string   | Query string in PQL format                 |
| count   | int      | Optional limit of logs to read initially   |
| endDate | string   | Start streaming from logs before this time |

#### Response

Returns EventStream of json objects like this.

data:

```json
{
"id": "123456789",
"timestamp": "2025-01-01T12:00:00",
"level": "trace" | "debug" | "info" | "warning" | "error" | "fatal",
	"props": [
		{
			"key": "key",
			"value": "value"
		}
	],
	"message": "Log message"
}
```

### GET /api/v1/logs/histogram

Streams log counts grouped into time buckets as Server-Sent Events.

#### Query

| Field      | DataType | Description                         |
| ---------- | -------- | ----------------------------------- |
| query      | string   | Query string in PQL format          |
| bucketSecs | int      | Bucket size in seconds (default 60) |

#### Response

Each event contains a JSON object:

```json
{
	"timestamp": "2025-01-01T12:00:00",
	"count": 1
}
```

### POST /api/v1/device/settings

Apply settings to many devices at once. Devices are matched based on metadata uploaded by devices.

**application/json**

```json
{
	"filter_props": [
		{
			"key": "model",
			"value": "x123"
		}
	],
	"send_logs": true,
	"send_interval": 60,
	"level": "LogLevel"
}
```

### GET /api/v1/device/:deviceId/status

Gets status for device. Usefull for determining if device is allowed to send logs or not and what logs should be sent. Client can use this api to keep TLS connection alive or makes sure not to waste bandwidth sending logs to server which is not ready to receive logs. In some environments like IOT devices it's important to save battery and bandwidth.

**application/json**

```json
{
	"level": "LogLevel" | null,
	"should_send_logs": true | false
}
```

### POST /api/v1/device/:deviceId/logs

Device sends logs to server in LogBatch format and server will send ack back to client with sequence number. After ack is received it is safe to remove batch from client memory. This api works both with chunked transfer encoding and normal post request.

Transfer-Encoding: chunked // If streaming logs
Content-Encoding: gzip, zstd, none

### POST /api/v1/settings

Set query used for collecting logs.

**text/plain**

```
level > warning
```

### GET /api/v1/settings

Returns current collection query as plain text.

### POST /api/v1/device/:deviceId/metadata

Devices can upload metadata about themselves to the server. When metadata is uploaded it replaces the old metadata.
This metadata is used for finding devices and is also useful when sending fleet commands to devices.

**application/json**

```json
[
	{
		"key": "key",
		"value": "value"
	}
]
```

### POST /api/v1/device/:deviceId/settings

Update settings for a single device.

**application/json**

```json
{
	"sendLogs": true,
	"filterLevel": "LogLevel",
	"sendInterval": 60
}
```

### POST /api/v1/device/bulkedit

Bulk edit settings for multiple devices identified by their ids.

**application/json**

```json
{
	"filterLevel": "LogLevel",
	"sendLogs": true,
	"sendInterval": 60,
	"deviceIds": ["123", "456"]
}
```

### GET /api/v1/devices

Returns list of known devices in json format.

### GET /api/v1/validate_query

Validate a PQL query string. Returns `200` if valid otherwise `400` with error.

#### Query

| Field | DataType | Description                |
| ----- | -------- | -------------------------- |
| query | string   | Query string in PQL format |

## Install

### Linux

```
sudo apt-get install gcc libssl-dev pkg-config
cargo run
```

### Command Line Interface

PuppyLog comes with a CLI called `puppylogcli`. The CLI automatically reads the
server address from the `PUPPYLOG_ADDRESS` environment variable or the file
`$HOME/.puppylog/address` when `--address` is not provided. Run `cargo run --bin
puppylogcli -- --help` to see all commands. Available commands include:

| Command                   | Description                                       |
| ------------------------- | ------------------------------------------------- |
| `upload`                  | Upload randomly generated logs to a server        |
| `tokenize drain`          | Tokenize a log file using the Drain algorithm     |
| `update-metadata`         | Upload updated device metadata from a JSON file   |
| `segment get`             | Query segment metadata using filters              |
| `segment download`        | Download segments to a directory                  |
| `segment download-remove` | Download segments and delete them from the server |
| `import`                  | Import compressed log segments from a directory   |

Example importing log segments:

```
cargo run --bin puppylogcli -- import ./segments
```

### Building the Web UI

The web interface is written in TypeScript. Bundle the assets with `bun` and
type‑check the sources using `tsc`:

```
bun build ./ts/app.ts --outfile=./assets/puppylog.js
bun x tsc --noEmit
```

## Configuration

PuppyLog supports tuning of its in-memory buffering and merge batching behavior via environment variables:

- **MERGER_MAX_IN_CORE**: Maximum number of log entries buffered in memory for device merging. Defaults to the compile-time constant `MAX_IN_CORE`.
- **MERGER_TARGET_SEGMENT_SIZE**: Number of buffered entries per device before triggering a flush to storage. Defaults to `TARGET_SEGMENT_SIZE`.
- **MERGER_BATCH_SIZE**: Number of orphan log segments fetched per merge iteration. Defaults to `MERGER_BATCH_SIZE`.
- **MERGER_RUN**: Enables or disables merger background processing. Defaults to `true`.
- **UPLOAD_FLUSH_THRESHOLD**: Number of buffered log entries received via the upload API before they are persisted to storage. The server reads this value at startup. Defaults to the compile-time constant `UPLOAD_FLUSH_THRESHOLD`.
- **RUN_SEGMENT_COMPACTOR**: Enables or disables the segment compactor background process. Defaults to `true`. It tries to compact device segment logs up to 300k entries per segment. This helps to improve compression ratio and query performance with less files on disk.

To override these at runtime, set the variables before starting the server, for example:

```bash
export MERGER_MAX_IN_CORE=1000000
export MERGER_TARGET_SEGMENT_SIZE=300000
export MERGER_BATCH_SIZE=2000
export UPLOAD_FLUSH_THRESHOLD=50000
```
