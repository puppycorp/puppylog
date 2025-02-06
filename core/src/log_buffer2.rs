use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Path;

pub const START_OFFSET: u64 = 16;

pub struct CircleBuffer {
    file: File,
    head: u64,
    tail: u64,
    capacity: u64,
    data_start: u64,
    buff: Vec<u8>,
}

impl CircleBuffer {
    pub fn new<P: AsRef<Path>>(path: P, capacity: u64) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let (head, tail) = if file.metadata()?.len() >= START_OFFSET {
            let mut head_bytes = [0u8; 8];
            let mut tail_bytes = [0u8; 8];
            file.seek(io::SeekFrom::Start(0))?;
            file.read_exact(&mut head_bytes)?;
            file.read_exact(&mut tail_bytes)?;
            (u64::from_le_bytes(head_bytes), u64::from_le_bytes(tail_bytes))
        } else {
            file.seek(io::SeekFrom::Start(0))?;
            file.write_all(&0u64.to_le_bytes())?;
            file.write_all(&0u64.to_le_bytes())?;
            (0, 0)
        };

        file.set_len(START_OFFSET + capacity)?;

        Ok(Self {
            file,
            head,
            tail,
            capacity,
            data_start: START_OFFSET,
            buff: Vec::new(),
        })
    }

    pub fn save(&mut self) -> io::Result<()> {
        if self.buff.is_empty() {
            return Ok(());
        }

        let buff_len = self.buff.len() as u64;
        let current_head = self.head;

        let (first_part, second_part) = if current_head + buff_len <= self.capacity {
            (&self.buff[..], &self.buff[0..0])
        } else {
            let split_at = (self.capacity - current_head) as usize;
            self.buff.split_at(split_at)
        };

        self.file.seek(io::SeekFrom::Start(self.data_start + current_head))?;
        self.file.write_all(first_part)?;

        if !second_part.is_empty() {
            self.file.seek(io::SeekFrom::Start(self.data_start))?;
            self.file.write_all(second_part)?;
        }

        let new_head = (current_head + buff_len) % self.capacity;
        self.head = new_head;

        self.file.seek(io::SeekFrom::Start(0))?;
        self.file.write_all(&self.head.to_le_bytes())?;
        self.file.write_all(&self.tail.to_le_bytes())?;

        self.buff.clear();

        Ok(())
    }
}

impl Write for CircleBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buff.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.save()
    }
}

impl Read for CircleBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = (self.head + self.capacity - self.tail) % self.capacity;
        if available == 0 {
            return Ok(0);
        }

        let to_read_total = std::cmp::min(available as usize, buf.len());
        let mut total_read = 0;
        let mut remaining = &mut buf[..to_read_total];

        while total_read < to_read_total {
            let current_tail = self.tail;
            let space_until_end = self.capacity - current_tail;
            let to_read = std::cmp::min(space_until_end as usize, remaining.len());

            self.file.seek(io::SeekFrom::Start(self.data_start + current_tail))?;
            let read = self.file.read(&mut remaining[..to_read])?;
            if read == 0 {
                break;
            }

            self.tail = (current_tail + read as u64) % self.capacity;
            total_read += read;
            remaining = &mut remaining[read..];
        }

        Ok(total_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn store() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test");
        let mut buffer = CircleBuffer::new(&path, 100)?;
        let data = b"Hello, world!";
        buffer.write_all(data)?;
        buffer.flush()?;

        let mut data2 = [0; 13];
        buffer.read_exact(&mut data2)?;
        assert_eq!(data, &data2);
        Ok(())
    }
}