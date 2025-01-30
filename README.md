# puppylog

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

Logline is a binary structure which stores log information.

| Field      | Size | Description                          |
|------------|------|--------------------------------------|
| Version    | 2	| Version of the logentry              |
| Timestamp  | 8    | Timestamp of the log                 |
| Random     | 4    | Ensure uniqueness within millisecond |
| Level      | 1    | Log level                            |
| PropsCount | 1    | Project identifier                   |
| Props      | x    | Properties of the logentry           |
| MsgLen     | 4    | Length of the message                |
| Message    | x    | Log message payload                  |

**Loglevel**

| Value | Description |
|-------|-------------|
| 0     | Debug       |
| 1     | Info        |
| 2     | Warning     |
| 3     | Error       |

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
| start     | DateTime | Start time for logs                   |
| end       | DateTime | End time for logs                     |
| order     | int      | Order of the logs                     |
| count     | int      | Number of logs to return (default 50) |
| loglevel  | enum[]   | Debug, Info, Warning, Error           |
| props	 	| string[] | Properties of the logentry            |
| search    | string[] | Message payload of the logmessage     |

#### Response

```json
[
    {
        "timestamp": "",
        "loglevel": 2,
        "project": 5,
        "env": 1,
        "device": 1234,
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
    "timestamp": "",
    "loglevel": 2,
    "project": 5,
    "env": 1,
    "device": 1234,
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

### POST /api/logs/{group}

Device can send batch of loglines to server in compressed format like tar.gz. Payload will have one or more loglines in specified format.

Content-Encoding: gzip or none

**Logline**

|Field      |Size|Description             |
|-----------|----|------------------------|
| timestamp | 8  | Timestamp of the log   |
| loglevel  | 1  | Log level              |
| project   | 4  | Project identifier     |
| env       | 4  | Environment identifier |
| device    | 4  | Device identifier      |
| msglen    | 4  | Length of the message  |
| message   | x  | Log message            |

### POST /api/logs/stream

Stream logs to server. Because this method has higher bandwidth usage it is recommended to use it only when needed. For example when debugging some issue.

Transfer-Encoding: chunked

```
size of logline\r\n
Logline(same format as normal post) \r\n
... more loglines
0\r\n
\r\n
```


### POST /api/device/{devid}/rawlogs

Post raw logs as they are stored in device. However this might require user to insert some processing rules if the log schema is not automatically detectable. There could be some basic asumptions like timestamp is in certain format or it is the first column.

Content-Type: text/plain
Content-Encoding: gzip or none

Logs in plain text format...

### POST /api/device/{devid}/rawlogs/stream

Stream raw logs to server. This is useful when logs are generated in real time and they are not stored in the device. This can be used to stream logs from the device to the server.

Transfer-Encoding: chunked

```
size of logline\r\n
Logline\r\n
... more loglines
0\r\n