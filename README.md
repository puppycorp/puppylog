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
|------------|------|--------------------------------------|
| Version    | 2	| Version of the logentry (current 1)  |
| Timestamp  | 8    | Timestamp of the log in micros       |
| Random     | 4    | Ensure uniqueness within microsecond |
| Level      | 1    | Log level                            |
| PropsCount | 1    | Property count                       |
| Props      | x    | Properties of the logentry           |
| MsgLen     | 4    | Length of the message                |
| Message    | x    | Log message payload                  |

**Loglevel**

| Value | Description |
|-------|-------------|
| 1     | Trace       |
| 2     | Debug       |
| 3     | Info        |
| 4     | Warning     |
| 5     | Error       |
| 6     | Fatal       |

**Property**

| Field  | Size | Description           |
|--------|------|-----------------------|
| KeyLen | 1	| Length of the key     |
| Key    | x    | Key of the property   |
| ValLen | 1    | Length of the value   |
| Value  | x    | Value of the property |

### LogBatch

Logbatch is a binary structure which stores multiple loglines. Logbatch is compressed with gzip or zstd. Logbatch is used to send multiple loglines to server in one go. Supported decryption by server is gzip and zstd.

| Field      | Size | Description                                  |
|------------|------|----------------------------------------------|
| Version    | 2	| Version of logbatch (current 1)              |
| Seq		 | 4    | Sequence number of the logbatch              |
| Crc32      | 4    | CRC32 checksum of the logbatch               |
| Size       | 4    | Payload size in bytes                        |
| LogEntries | x    | LogEntries in binary format                  |

## API

### GET /api/v1/logs

Search logs with PQL query. Returns logs in json format. 

#### Query

| Field     | DataType | Description                           |
| --------- | -------- | --------------------------------------|
| offset	| int      | Offset of the logs                    |
| count     | int      | Number of logs to return (default 200)|
| query     | string   | Query string in PQL format            |

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

### GET /api/v1/logs/stream

#### Query

| Field     | DataType | Description                       |
| --------- | -------- | --------------------------------- |
| query     | string   | Query string in PQL format        |

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

### POST /api/v1/devices/settings
Apply settings to many devices at once. Devices are matched based on metadata uploaded by devices.

**application/json**
```json
{
	"query": "PGL query string",
	"minInterval": 60,
	"maxInterval": 3600,
	"level": "LogLevel" | null,
}
```


### WS /api/v1/device/:deviceId/ws

**Binary**
Client will send `LogBatch` in binary format to server and when server has processed it will send ack back to client with sequence number. After ack is recevided client can remove batch from it's memory.

**application/json**
Server send json encoded mesages to client in one of these formats:
```json
{
	"type": "settings",
	"settings": "SettingsObject"
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

### /api/v1/settings

**application/json**
```json
{
	"query": "PGL query string",
	"minInterval": 60,
	"maxInterval": 3600,
	"level": "LogLevel" | null,
	"max_logfile_size": 1000000, // 1MB
	"max_logfile_count": 5,
}
```

Post will set settings and get will get settings.

## POST /api/v1/dvice/:deviceId/metadata

Devices can upload metadata metadata of them to server. When metadata is uploaded it replaces the old metadata.
This metadata is used for finding devices and also usefull when sending fleet commands to devices.

**application/json**
```json
[
	{
		"key": "key",
		"value": "value"
	}
]
```


## Install

### Linux

```
sudo apt-get install gcc libssl-dev pkg-config
cargo run
```

