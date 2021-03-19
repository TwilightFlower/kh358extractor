use bytes::Bytes;
use std::{
	ptr,
	ops::Deref,
	marker::PhantomData,
	time::Instant,
	mem::forget
};
use cty::{c_char, c_long, c_int};

pub struct RawBuf {
	slice_ptr: *const [u8],
	_p: PhantomData<[u8]>
}

impl Deref for RawBuf {
	type Target = [u8];
	fn deref(&self) -> &[u8] {
		unsafe{&*self.slice_ptr}
	}
}

impl Drop for RawBuf {
	fn drop(&mut self) {
		unsafe{
			let ptr_as_char = &(*self.slice_ptr)[0] as *const u8 as usize as *const c_char;
			//println!("Attempting to dealloc");
			//deallocBuf(ptr_as_char)
		}
	}
}

pub fn safe_compress(in_buf: &[u8]) -> Result<Vec<u8>, String> {
	let len = in_buf.len() as c_long;
	let buf_ptr = &in_buf[0] as *const u8 as usize as *const c_char; // lol
	let stime = Instant::now();
	//println!("Compressing buf with length {}", len);
	let result = unsafe{compress(buf_ptr, len, create_buf)};
	//println!("Compressing took {} ms", stime.elapsed().as_millis());
	if result.retcode == 0 {
		let new_ptr = result.buf as *mut u8;
		/*let slice = ptr::slice_from_raw_parts(new_ptr, result.length as usize);
		Ok(RawBuf{slice_ptr: slice, _p: PhantomData})*/
		Ok(unsafe{Vec::from_raw_parts(new_ptr, result.length as usize, result.length as usize)})
	} else {
		Err(format!("Compression error {}", result.retcode))
	}
}

#[repr(C)]
struct CompressionResult {
	buf: *const c_char,
	length: c_long,
	retcode: c_int
}

extern "C" fn create_buf(len: c_long) -> *mut c_char {
	let mut vec = Vec::with_capacity(len as usize);
	let ptr = vec.as_mut_ptr();
	forget(vec);
	ptr
}

#[link(name = "compress")]
extern "C" {
	fn compress(in_buf: *const c_char, len: c_long, buf_creator: extern "C" fn(c_long) -> *mut c_char) -> CompressionResult;
}
