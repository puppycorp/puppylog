# PuppyLog - Make Logging Great Again

PuppyLog is log collection server where clients can submit logs and the send queries to get logs. Log queries are send in Puppy Query Language (PQL) format. Server supports streaming logs and querying logs. Protocol is designed to be efficient and easy to implement in different environments like server, desktop, mobile and IOT devices.

## PQL - Puppy Query Language

**Compare Operators**
```
< // Smaller than
> // Larger tha
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
| PropsCount | 1    | Project identifier                   |
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

## API

### GET /api/logs
Get logs

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

### GET /api/logs/stream

#### Query

| Field     | DataType | Description                       |
| --------- | -------- | --------------------------------- |
| loglevel  | enum[]   | Debug, Info, Warning, Error       |
| props	 	| string[] | Properties of the logentry        |
| search    | string[] | Message payload of the logmessage |

#### Response
Returns eventstream of json objects like this.

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



### GET /api/commands
Event stream which receives commands from server. This can be used to control the clients like do they send logs or not. In some environments data amount need to be restricted like IOT devices so log sending can be turned on demand.

**Command**

|Field       |Size|Description             |
|------------|----|------------------------|
|type        | 1  | Type of command        |
|payload len | 4  | Payload of the command |

**Stream Command**

**Send logs**

| Field       | Size | Description              |
|-------------|------|--------------------------|
| Start date  | 8    | Earliest logline to send |
| End date    | 8    | Lastest logline to send  |

### WS /api/device/:deviceId/ws

### GET /api/settings/query

Get log collection query from server so that clients can filter logs on device side.

**Body**
Query string

### POST /api/settings/query

Send log collection query to clients so that they can filter logs on device side.

**Body**
Query string


### POST /api/logs

Device can send batch of loglines to server in compressed format like tar.gz. Payload will have one or more loglines in specified format. Supports gzip and zstd compression. Also supports streaming logs with chunked transfer encoding.

Transfer-Encoding: chunked // If streaming logs
Content-Encoding: gzip, zstd, none

Back to back list of loglines in binary format.

### POST /api/ping

Ping endpoint to check if server is ready to receive logs. Returns 200 OK if server is ready to receive logs. Client can use this to keep TLS connection alive or makes sure not to waste bandwidth sending logs to server which is not ready to receive logs. In some environments like IOT devices it's important to save battery and bandwidth.