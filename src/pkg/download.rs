//! Downloads through Foundation's URL loading, the system's own HTTP stack
//! (its certificates, proxies and App Transport Security), on the shell's
//! thread, which waits for the answer.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use crate::objc::{self, NIL, Object, Raw};

/// `NSURLRequestReloadIgnoringLocalCacheData`: always ask the server.
const RELOAD: usize = 1;
/// Seconds to wait for a server that stops answering.
const TIMEOUT: f64 = 60.0;

/// The body of `url`, refused unless the server answered 2xx, or if it is
/// larger than `limit`.
pub(super) fn get(url: &str, limit: usize) -> Result<Vec<u8>, String> {
	let _pool = objc::Pool::new();
	let missing = |error: objc::Missing| error.to_string();
	let string = objc::string(url).ok_or(format!("{url}: not a URL"))?;
	let urls = objc::class(c"NSURL").map_err(missing)?;
	// SAFETY: `URLWithString:` takes an `NSString` and returns an
	// autoreleased `NSURL`, or nil for text that is not a URL.
	let address: Raw = unsafe { urls.send(c"URLWithString:", (string.raw(),)) };
	if address.is_null() {
		return Err(format!("{url}: not a URL"));
	}
	let requests = objc::class(c"NSURLRequest").map_err(missing)?;
	// SAFETY: `requestWithURL:cachePolicy:timeoutInterval:` takes an
	// `NSURL`, an `NSURLRequestCachePolicy` (`NSUInteger`) and an
	// `NSTimeInterval` (`double`), and returns an autoreleased request.
	let request: Raw = unsafe {
		requests.send(
			c"requestWithURL:cachePolicy:timeoutInterval:",
			(address, RELOAD, TIMEOUT),
		)
	};
	let connections = objc::class(c"NSURLConnection").map_err(missing)?;
	let (mut response, mut error): (Raw, Raw) = (NIL, NIL);
	// SAFETY: `sendSynchronousRequest:returningResponse:error:` takes a
	// request and two pointers it may store autoreleased objects through,
	// both live locals; it returns when the download ends, with an
	// autoreleased `NSData` or nil.
	let data: Raw = unsafe {
		connections.send(
			c"sendSynchronousRequest:returningResponse:error:",
			(request, &raw mut response, &raw mut error),
		)
	};
	// SAFETY: `data` is nil or the live `NSData` just returned.
	let Some(data) = (unsafe { Object::retained(data) }) else {
		// SAFETY: `error` is nil or the `NSError` stored above.
		return Err(format!("{url}: {}", unsafe {
			objc::describe(error, "the download failed")
		}));
	};
	if let Some(status) = status(response)
		&& !(200..300).contains(&status)
	{
		return Err(format!("{url}: the server answered HTTP {status}"));
	}
	// SAFETY: `length` takes nothing and returns the `NSUInteger` count.
	let length: usize = unsafe { data.send(c"length", ()) };
	if length > limit {
		return Err(format!("{url}: larger than {limit} bytes"));
	}
	if length == 0 {
		return Ok(Vec::new());
	}
	// SAFETY: `bytes` returns the data's `length` bytes, which live as
	// long as `data`, retained until the copy below is made.
	let bytes: *const u8 = unsafe { data.send(c"bytes", ()) };
	// SAFETY: as above, `length` readable bytes at `bytes`.
	Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
}

/// The HTTP status of `response`, nil or an `NSURLResponse`, if it has one.
fn status(response: Raw) -> Option<isize> {
	// SAFETY: `response` is nil or the `NSURLResponse` stored by the
	// connection.
	let response = unsafe { Object::retained(response) }?;
	if !response.responds(c"statusCode") {
		return None;
	}
	// SAFETY: an `NSHTTPURLResponse`'s `statusCode` returns an `NSInteger`.
	Some(unsafe { response.send(c"statusCode", ()) })
}
