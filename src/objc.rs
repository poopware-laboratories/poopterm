//! The Objective-C runtime, through which Metal and Core Animation are
//! reached: classes and selectors looked up by name, messages sent through
//! `objc_msgSend` cast to each method's own type, objects owned by one
//! reference that is released when dropped, strings, and autorelease pools.
// Messages still share blocks; the Metal renderer work wraps each in its
// own safe method and removes this.
#![expect(clippy::multiple_unsafe_ops_per_block)]
use std::{
	ffi::{CStr, c_char, c_void},
	fmt,
	marker::PhantomData,
	ptr::{self, NonNull},
};

/// An object or class as the runtime passes it; null is nil.
pub(crate) type Raw = *mut c_void;
/// `BOOL`: `bool` on arm64, `signed char` on x86-64, a byte either way;
/// read as zero or not.
pub(crate) type Bool = i8;

#[link(name = "objc")]
unsafe extern "C" {
	fn objc_getClass(name: *const c_char) -> Raw;
	fn sel_registerName(name: *const c_char) -> Sel;
	/// Declared without a type: each call casts it to its method's.
	fn objc_msgSend();
	fn objc_retain(object: Raw) -> Raw;
	fn objc_release(object: Raw);
	fn objc_autoreleasePoolPush() -> Raw;
	fn objc_autoreleasePoolPop(pool: Raw);
	fn objc_allocateClassPair(superclass: Raw, name: *const c_char, extra: usize) -> Raw;
	fn objc_registerClassPair(class: Raw);
	fn class_addMethod(class: Raw, name: Sel, imp: *const c_void, types: *const c_char) -> Bool;
}

/// An action method's implementation: the receiver, the selector, and the
/// sender, returning nothing, as AppKit sends menu items' actions.
pub(crate) type Action = extern "C" fn(Raw, Sel, Raw);

/// `NSUTF8StringEncoding`.
const UTF8: usize = 4;

/// A class the runtime lacks, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Missing(pub &'static CStr);

impl fmt::Display for Missing {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "no Objective-C class {}", self.0.to_string_lossy())
	}
}

/// A selector: a method's name, registered with the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct Sel(*const c_void);

impl Sel {
	/// No selector, as an item without an action has.
	pub(crate) const NULL: Self = Self(ptr::null());
}

/// The selector named `name`.
pub(crate) fn sel(name: &CStr) -> Sel {
	// SAFETY: `name` is a C string; the runtime registers it if new and
	// returns the same selector for the same name, never null.
	unsafe { sel_registerName(name.as_ptr()) }
}

/// A class, which lives as long as the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Class(NonNull<c_void>);

/// The class named `name`, if the runtime has one.
pub(crate) fn class(name: &'static CStr) -> Result<Class, Missing> {
	// SAFETY: `name` is a C string; the runtime returns the class or nil.
	let raw = unsafe { objc_getClass(name.as_ptr()) };
	NonNull::new(raw).map(Class).ok_or(Missing(name))
}

impl Class {
	pub(crate) fn raw(self) -> Raw {
		self.0.as_ptr()
	}

	/// Sends the class `selector` with `arguments`.
	///
	/// # Safety
	///
	/// The class has a method `selector` that takes `arguments`' types, in
	/// order, and returns `R`.
	pub(crate) unsafe fn send<R>(self, selector: &CStr, arguments: impl Arguments) -> R {
		// SAFETY: as the caller vouches; a class is never nil.
		unsafe { arguments.send(self.raw(), sel(selector)) }
	}

	/// A new instance, `[[class alloc] init]` through `new`; `None` if it
	/// returned nil.
	///
	/// # Safety
	///
	/// The class descends from `NSObject` and `init` needs no arguments.
	pub(crate) unsafe fn instance(self) -> Option<Object> {
		// SAFETY: as the caller vouches; `new` returns a reference the
		// caller owns.
		unsafe { Object::owned(self.send(c"new", ())) }
	}
}

/// A new class named `name`, descending from `NSObject`, whose instances
/// answer `selector` with `action`; the class already named `name` if
/// there is one, which is taken to be this one made earlier.
pub(crate) fn action_class(
	name: &'static CStr,
	selector: &CStr,
	action: Action,
) -> Result<Class, Missing> {
	let superclass = class(c"NSObject")?;
	// SAFETY: the superclass is a class and `name` a C string; the runtime
	// returns a new, unregistered class, or nil if the name is taken.
	let made = unsafe { objc_allocateClassPair(superclass.raw(), name.as_ptr(), 0) };
	if made.is_null() {
		return class(name);
	}
	// SAFETY: `made` is unregistered, so methods may be added; `v@:@`
	// encodes `action`'s type: returning void, taking the receiver, the
	// selector and an object. Registering it then makes it usable.
	unsafe {
		class_addMethod(
			made,
			sel(selector),
			action as *const c_void,
			c"v@:@".as_ptr(),
		);
		objc_registerClassPair(made);
	}
	class(name)
}

/// The arguments of a message, after its receiver and selector.
pub(crate) trait Arguments {
	/// Sends `receiver` `selector` with these arguments.
	///
	/// # Safety
	///
	/// `receiver` is a live object or class with a method `selector` that
	/// takes these arguments' types, in order, and returns `R`.
	unsafe fn send<R>(self, receiver: Raw, selector: Sel) -> R;
}

macro_rules! arguments {
	($($name:ident: $type:ident),*) => {
		impl<$($type),*> Arguments for ($($type,)*) {
			unsafe fn send<R>(self, receiver: Raw, selector: Sel) -> R {
				let ($($name,)*) = self;
				type Typed<R $(, $type)*> = unsafe extern "C" fn(Raw, Sel $(, $type)*) -> R;
				let send: Typed<R $(, $type)*>;
				// SAFETY: `objc_msgSend` jumps to the method's implementation
				// with the arguments as the caller passed them, so called
				// as the method's own C type it is that method; both are
				// function pointers of one size.
				send = unsafe {
					std::mem::transmute::<unsafe extern "C" fn(), Typed<R $(, $type)*>>(objc_msgSend)
				};
				// SAFETY: as the caller vouches.
				unsafe { send(receiver, selector $(, $name)*) }
			}
		}
	};
}

arguments!();
arguments!(a: A);
arguments!(a: A, b: B);
arguments!(a: A, b: B, c: C);
arguments!(a: A, b: B, c: C, d: D);
arguments!(a: A, b: B, c: C, d: D, e: E);
arguments!(a: A, b: B, c: C, d: D, e: E, f: F);

/// An object, owned by one reference that is released when dropped; a
/// clone retains another.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Object(NonNull<c_void>);

impl Object {
	/// Takes over the reference to `raw` that its caller owns, as a method
	/// whose name starts `alloc`, `new`, `copy` or `mutableCopy` returns;
	/// `None` for nil.
	///
	/// # Safety
	///
	/// `raw` is nil or a live object descending from `NSObject` whose
	/// reference the caller owns and hands over.
	pub(crate) unsafe fn owned(raw: Raw) -> Option<Self> {
		NonNull::new(raw).map(Self)
	}

	/// Retains `raw`, as returned by any other method, which the caller
	/// does not own; `None` for nil.
	///
	/// # Safety
	///
	/// `raw` is nil or a live object descending from `NSObject`.
	pub(crate) unsafe fn retained(raw: Raw) -> Option<Self> {
		// SAFETY: as the caller vouches; retaining nil returns nil.
		let raw = unsafe { objc_retain(raw) };
		NonNull::new(raw).map(Self)
	}

	pub(crate) fn raw(&self) -> Raw {
		self.0.as_ptr()
	}

	/// Sends the object `selector` with `arguments`.
	///
	/// # Safety
	///
	/// The object has a method `selector` that takes `arguments`' types,
	/// in order, and returns `R`.
	pub(crate) unsafe fn send<R>(&self, selector: &CStr, arguments: impl Arguments) -> R {
		// SAFETY: as the caller vouches; the object is live while `self` is.
		unsafe { arguments.send(self.raw(), sel(selector)) }
	}

	/// Whether the object has a method `selector`, which messages it lacks
	/// would raise an exception for.
	pub(crate) fn responds(&self, selector: &CStr) -> bool {
		// SAFETY: every `NSObject` answers `respondsToSelector:`, which
		// takes a selector and returns a `BOOL`.
		let responds: Bool = unsafe { self.send(c"respondsToSelector:", (sel(selector),)) };
		responds != 0
	}
}

impl Clone for Object {
	fn clone(&self) -> Self {
		// SAFETY: the object is live; retaining it returns it.
		unsafe { objc_retain(self.raw()) };
		Self(self.0)
	}
}

impl Drop for Object {
	fn drop(&mut self) {
		// SAFETY: `self` owns one reference to the object, given up here.
		unsafe { objc_release(self.raw()) };
	}
}

/// An autorelease pool: objects autoreleased while it is the innermost
/// are released when it drops. Pools nest, and drop in reverse order, as
/// one is made and dropped on one thread.
pub(crate) struct Pool {
	handle: Raw,
	/// Kept on the thread that pushed it.
	_thread: PhantomData<*mut ()>,
}

impl Pool {
	pub(crate) fn new() -> Self {
		// SAFETY: pushing a pool has no preconditions.
		let handle = unsafe { objc_autoreleasePoolPush() };
		Self {
			handle,
			_thread: PhantomData,
		}
	}
}

impl Drop for Pool {
	fn drop(&mut self) {
		// SAFETY: the handle came from a push on this thread, and pools
		// inside it, made later, dropped first.
		unsafe { objc_autoreleasePoolPop(self.handle) };
	}
}

/// `text` as an `NSString`, owned; `None` if one could not be made.
pub(crate) fn string(text: &str) -> Option<Object> {
	let class = class(c"NSString").ok()?;
	// SAFETY: `alloc` returns an uninitialized string the caller owns,
	// which `initWithBytes:length:encoding:` consumes, returning the
	// initialized one or nil; the bytes outlive the call and are UTF-8.
	unsafe {
		let blank: Raw = class.send(c"alloc", ());
		let made: Raw =
			(text.as_ptr(), text.len(), UTF8).send(blank, sel(c"initWithBytes:length:encoding:"));
		Object::owned(made)
	}
}

/// The text of `string`, an `NSString`, copied.
///
/// # Safety
///
/// `string` is an `NSString`.
pub(crate) unsafe fn text(string: &Object) -> String {
	// SAFETY: as the caller vouches; `UTF8String` returns a C string, or
	// null, that lives at least as long as the string and the innermost
	// autorelease pool, and it is copied at once.
	let chars: *const c_char = unsafe { string.send(c"UTF8String", ()) };
	if chars.is_null() {
		return String::new();
	}
	// SAFETY: as above, a C string.
	unsafe { CStr::from_ptr(chars) }
		.to_string_lossy()
		.into_owned()
}

/// What `error`, an `NSError` or nil, says went wrong; `otherwise` for nil.
///
/// # Safety
///
/// `error` is nil or an `NSError`.
pub(crate) unsafe fn describe(error: Raw, otherwise: &str) -> String {
	// SAFETY: as the caller vouches.
	let Some(error) = (unsafe { Object::retained(error) }) else {
		return otherwise.into();
	};
	// SAFETY: `localizedDescription` returns an `NSString`, never nil.
	let raw: Raw = unsafe { error.send(c"localizedDescription", ()) };
	// SAFETY: as above.
	match unsafe { Object::retained(raw) } {
		// SAFETY: an `NSString`.
		Some(description) => unsafe { text(&description) },
		None => otherwise.into(),
	}
}

/// Nil, for arguments that may be.
pub(crate) const NIL: Raw = ptr::null_mut();

#[cfg(test)]
#[path = "objc_probe_tests.rs"]
pub(crate) mod probe_tests;
#[cfg(test)]
#[path = "objc_tests.rs"]
mod tests;
