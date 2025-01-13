use std::mem::MaybeUninit;

#[derive(Debug)]
pub enum BufferError {
	Full,
	Empty
}

pub struct CircularBuffer<T> {
	buffer: Vec<MaybeUninit<T>>,
	head: usize,
	new_head: usize,
	tail: usize,
	size: usize,
	new_size: usize,
	capacity: usize,
}

impl<T> CircularBuffer<T> {
	pub fn new(capacity: usize) -> Self {
		let mut buffer: Vec<MaybeUninit<T>> = Vec::with_capacity(capacity);
		unsafe {
			buffer.set_len(capacity);
		}
		CircularBuffer {
			buffer,
			head: 0,
			new_head: 0,
			tail: 0,
			size: 0,
			new_size: 0,
			capacity,
		}
	}

	pub fn push(&mut self, value: T) -> Result<(), BufferError> {
		if self.is_full() {
			return Err(BufferError::Full);
		}
		self.buffer[self.tail] = MaybeUninit::new(value);
		self.tail = (self.tail + 1) % self.capacity;
		self.size += 1;
		self.new_size += 1;
		Ok(())
	}

	pub fn pop(&mut self) -> Result<T, BufferError> {
		if self.is_empty() {
			return Err(BufferError::Empty);
		}
		let item = unsafe {
			self.buffer[self.new_head].assume_init_read()
		};
		self.new_head = (self.new_head + 1) % self.capacity;
		self.new_size -= 1;
		Ok(item)
	}
	
	pub fn commit_read(&mut self) {
		self.head = self.new_head;
		self.size = self.new_size;
	}

	pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn is_full(&self) -> bool {
        self.size == self.capacity
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Drop for CircularBuffer<T> {
	fn drop(&mut self) {
		while let Ok(_) = self.pop() {}
	}
}

impl std::io::Read for CircularBuffer<u8> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		let mut read = 0;
		while read < buf.len() && !self.is_empty() {
			buf[read] = self.pop().unwrap();
			read += 1;
		}
		Ok(read)
	}	
}

#[cfg(test)]
mod tests {
	#[test]
	fn test_circular_buffer() {
		let mut buffer = super::CircularBuffer::new(5);
		assert_eq!(buffer.capacity(), 5);
		assert_eq!(buffer.len(), 0);
		assert!(buffer.is_empty());
		assert!(!buffer.is_full());
		buffer.push(1).unwrap();
		buffer.push(2).unwrap();
		buffer.push(3).unwrap();
		buffer.push(4).unwrap();
		buffer.push(5).unwrap();
		assert_eq!(buffer.len(), 5);
		assert!(!buffer.is_empty());
		assert!(buffer.is_full());
		assert_eq!(buffer.pop().unwrap(), 1);
		assert_eq!(buffer.pop().unwrap(), 2);
		assert_eq!(buffer.pop().unwrap(), 3);
		assert_eq!(buffer.pop().unwrap(), 4);
		assert_eq!(buffer.pop().unwrap(), 5);
		assert_eq!(buffer.len(), 5);
		assert!(!buffer.is_empty());
		assert!(buffer.is_full());
		buffer.commit_read();
		assert_eq!(buffer.len(), 0);
		assert!(buffer.is_empty());
		assert!(!buffer.is_full());
	}
}