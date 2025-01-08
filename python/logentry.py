from dataclasses import dataclass
from typing import List, Dict
import struct
import time

@dataclass
class Property:
    key: str
    value: str
    
    def pack(self) -> bytes:
        key_bytes = self.key.encode('utf-8')
        value_bytes = self.value.encode('utf-8')
        
        return struct.pack(
            f'B{len(key_bytes)}sB{len(value_bytes)}s',
            len(key_bytes),
            key_bytes,
            len(value_bytes),
            value_bytes
        )
    
    @classmethod
    def unpack(cls, data: bytes, offset: int = 0) -> tuple['Property', int]:
        key_len = struct.unpack_from('B', data, offset)[0]
        offset += 1
        
        key = struct.unpack_from(f'{key_len}s', data, offset)[0].decode('utf-8')
        offset += key_len
        
        val_len = struct.unpack_from('B', data, offset)[0]
        offset += 1
        
        value = struct.unpack_from(f'{val_len}s', data, offset)[0].decode('utf-8')
        offset += val_len
        
        return cls(key, value), offset

class LogLevel:
    DEBUG = 0
    INFO = 1
    WARNING = 2
    ERROR = 3
    
    @classmethod
    def to_string(cls, level: int) -> str:
        return {
            cls.DEBUG: "Debug",
            cls.INFO: "Info", 
            cls.WARNING: "Warning",
            cls.ERROR: "Error"
        }.get(level, "Unknown")

@dataclass
class LogEntry:
    timestamp: int  # 8 bytes
    level: int      # 1 byte
    properties: List[Property]
    message: str
    
    def pack(self) -> bytes:
        props_bytes = b''.join(prop.pack() for prop in self.properties)
        msg_bytes = self.message.encode('utf-8')
        
        return struct.pack(
            f'=QBB{len(props_bytes)}sI{len(msg_bytes)}s',
            self.timestamp,
            self.level,
            len(self.properties),
            props_bytes,
            len(msg_bytes),
            msg_bytes
        )
    
    @classmethod
    def unpack(cls, data: bytes) -> 'LogEntry':
        offset = 0
        
        # Unpack fixed-length header
        timestamp, level, props_count = struct.unpack_from('=QBB', data, offset)
        offset += struct.calcsize('=QBB')
        
        # Unpack properties
        properties = []
        for _ in range(props_count):
            prop, new_offset = Property.unpack(data, offset)
            properties.append(prop)
            offset = new_offset
        
        # Unpack message length and message
        msg_len = struct.unpack_from('=I', data, offset)[0]
        offset += struct.calcsize('=I')
        
        message = struct.unpack_from(f'{msg_len}s', data, offset)[0].decode('utf-8')
        
        return cls(timestamp, level, properties, message)