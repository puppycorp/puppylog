# puppylog

## API

```
GET /api/logs?start,end,limit,offset,tag,search
GET /api/logs/stream?tag,search

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

### POST /api/logs

targz
```
TAG=myapp
TAG=devnumber

LOGLINE1\n
LOGLINE2\n
LOGLINE3\n
```

### POST /api/logs/raw

Post raw logs as they are stored in device. However this might require user to insert some processing rules if the log schema is not automatically detectable. There could be some basic asumptions like timestamp is in certain format or it is the first column.