use std::path::Path;

#[cfg(unix)]
pub fn available_space(path: &Path) -> u64 {
	use std::ffi::CString;
	use std::os::unix::ffi::OsStrExt;

	let path = match path.canonicalize() {
		Ok(p) => p,
		Err(_) => path.to_path_buf(),
	};
	let c_path = match CString::new(path.as_os_str().as_bytes()) {
		Ok(p) => p,
		Err(_) => return 0,
	};

	unsafe {
		let mut stat: libc::statvfs = std::mem::zeroed();
		if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
			(stat.f_bavail as u64) * (stat.f_bsize as u64)
		} else {
			0
		}
	}
}

#[cfg(windows)]
pub fn available_space(path: &Path) -> u64 {
	use std::ffi::OsStr;
	use std::iter::once;
	use std::os::windows::ffi::OsStrExt;
	use std::ptr::null_mut;

	#[link(name = "kernel32")]
	extern "system" {
		fn GetDiskFreeSpaceExW(
			lpDirectoryName: *const u16,
			lpFreeBytesAvailable: *mut u64,
			lpTotalNumberOfBytes: *mut u64,
			lpTotalNumberOfFreeBytes: *mut u64,
		) -> i32;
	}

	let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(once(0)).collect();
	let mut free: u64 = 0;
	unsafe {
		let res = GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, null_mut(), null_mut());
		if res == 0 {
			0
		} else {
			free
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;
	use tempfile::tempdir;

	#[test]
	fn available_for_existing() {
		let dir = tempdir().unwrap();
		let space = available_space(dir.path());
		assert!(space > 0);
	}

	#[test]
	fn available_for_missing_returns_zero() {
		let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
		path.push("nonexistent_path_should_not_exist");
		if path.exists() {
			std::fs::remove_dir_all(&path).unwrap();
		}
		let space = available_space(&path);
		assert_eq!(space, 0);
	}
}
