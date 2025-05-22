use std::sync::atomic::Ordering;

pub struct UploadGuard<'a> {
	pub counter: &'a std::sync::atomic::AtomicUsize,
}

impl<'a> UploadGuard<'a> {
	pub fn new(counter: &'a std::sync::atomic::AtomicUsize, max: usize) -> Result<Self, &'static str> {
		let prev = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |curr| {
			if curr >= max {
				None
			} else {
				Some(curr + 1)
			}
		});
		match prev {
			Ok(_) => Ok(Self { counter }),
			Err(_) => Err("Too many concurrent uploads")
		}
	}
}

impl Drop for UploadGuard<'_> {
	fn drop(&mut self) {
		self.counter.fetch_sub(1, Ordering::SeqCst);
	}
}