//! libwayland-client, opened at run time: the functions that send requests
//! and dispatch events for proxies on our own event queue, and the
//! interfaces of the core objects it exports. Requests and events use the
//! array forms, which are not variadic.
use crate::library::Library;
use std::{
	ffi::{CStr, c_char, c_int, c_void},
	mem::transmute_copy,
	ptr::NonNull,
};

/// The library's name on Linux and FreeBSD.
pub(crate) const LIBRARY: &CStr = c"libwayland-client.so.0";

/// A `wl_proxy`, `wl_display` included.
pub(crate) type Proxy = *mut c_void;
/// A `wl_event_queue`.
pub(crate) type Queue = *mut c_void;

/// A `wl_interface`, only ever passed by address.
#[repr(C)]
pub(crate) struct Interface {
	_opaque: [u8; 0],
}

/// `union wl_argument`: one argument of a request or event.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union Argument {
	pub int: i32,
	pub uint: u32,
	pub string: *const c_char,
	pub object: Proxy,
	/// A file descriptor, which sending duplicates.
	pub fd: i32,
	/// Room for the `wl_array` pointer and fixed-point forms.
	pub _wide: u64,
}

/// `wl_dispatcher_func_t`: called for each event on a proxy.
pub(crate) type Dispatcher = unsafe extern "C" fn(
	data: *const c_void,
	target: Proxy,
	opcode: u32,
	message: *const c_void,
	arguments: *mut Argument,
) -> c_int;

/// `WL_MARSHAL_FLAG_DESTROY`: the request destroys its proxy.
pub(crate) const DESTROY: u32 = 1;

pub(crate) type CreateQueue = unsafe extern "C" fn(Proxy) -> Queue;
pub(crate) type DestroyQueue = unsafe extern "C" fn(Queue);
pub(crate) type CreateWrapper = unsafe extern "C" fn(Proxy) -> Proxy;
pub(crate) type DestroyWrapper = unsafe extern "C" fn(Proxy);
pub(crate) type SetQueue = unsafe extern "C" fn(Proxy, Queue);
pub(crate) type Marshal =
	unsafe extern "C" fn(Proxy, u32, *const Interface, u32, u32, *mut Argument) -> Proxy;
pub(crate) type AddDispatcher =
	unsafe extern "C" fn(Proxy, Dispatcher, *const c_void, *mut c_void) -> c_int;
pub(crate) type DestroyProxy = unsafe extern "C" fn(Proxy);
pub(crate) type GetVersion = unsafe extern "C" fn(Proxy) -> u32;
/// `wl_display_dispatch_queue_pending` and `wl_display_roundtrip_queue`.
pub(crate) type DispatchQueue = unsafe extern "C" fn(Proxy, Queue) -> c_int;
pub(crate) type Flush = unsafe extern "C" fn(Proxy) -> c_int;

pub(crate) struct Client {
	pub create_queue: CreateQueue,
	pub destroy_queue: DestroyQueue,
	pub create_wrapper: CreateWrapper,
	pub destroy_wrapper: DestroyWrapper,
	pub set_queue: SetQueue,
	pub marshal: Marshal,
	pub add_dispatcher: AddDispatcher,
	pub destroy_proxy: DestroyProxy,
	pub version: GetVersion,
	pub dispatch_pending: DispatchQueue,
	pub roundtrip: DispatchQueue,
	pub flush: Flush,
	pub registry: *const Interface,
	pub shm: *const Interface,
	pub shm_pool: *const Interface,
	pub buffer: *const Interface,
	/// Kept open while its functions are in use.
	_library: Option<Library>,
}

impl Client {
	/// The system's libwayland-client.
	pub(crate) fn load() -> Result<Self, String> {
		let library = Library::open(LIBRARY)?;
		let lookup = |name: &CStr| library.symbol(name);
		let client = Self::find(lookup)?;
		Ok(Self {
			_library: Some(library),
			..client
		})
	}

	/// The client whose symbols `lookup` finds, or the first it lacks.
	pub(crate) fn find(lookup: impl Fn(&CStr) -> Option<NonNull<c_void>>) -> Result<Self, String> {
		let lookup = &lookup;
		let interface =
			|name: &CStr| symbol(lookup, name).map(|address| address.as_ptr().cast_const().cast());
		let function = |name| symbol(lookup, name);
		// SAFETY: libwayland-client exports each function with the type of
		// the field it goes to, as wayland-client-core.h declares it.
		unsafe {
			Ok(Self {
				create_queue: cast(function(c"wl_display_create_queue")?),
				destroy_queue: cast(function(c"wl_event_queue_destroy")?),
				create_wrapper: cast(function(c"wl_proxy_create_wrapper")?),
				destroy_wrapper: cast(function(c"wl_proxy_wrapper_destroy")?),
				set_queue: cast(function(c"wl_proxy_set_queue")?),
				marshal: cast(function(c"wl_proxy_marshal_array_flags")?),
				add_dispatcher: cast(function(c"wl_proxy_add_dispatcher")?),
				destroy_proxy: cast(function(c"wl_proxy_destroy")?),
				version: cast(function(c"wl_proxy_get_version")?),
				dispatch_pending: cast(function(c"wl_display_dispatch_queue_pending")?),
				roundtrip: cast(function(c"wl_display_roundtrip_queue")?),
				flush: cast(function(c"wl_display_flush")?),
				registry: interface(c"wl_registry_interface")?,
				shm: interface(c"wl_shm_interface")?,
				shm_pool: interface(c"wl_shm_pool_interface")?,
				buffer: interface(c"wl_buffer_interface")?,
				_library: None,
			})
		}
	}
}

/// The address of `name`, or an error naming it.
fn symbol(
	lookup: &dyn Fn(&CStr) -> Option<NonNull<c_void>>,
	name: &CStr,
) -> Result<NonNull<c_void>, String> {
	lookup(name).ok_or_else(|| {
		format!(
			"{} lacks {}",
			LIBRARY.to_string_lossy(),
			name.to_string_lossy()
		)
	})
}

/// The function at `address` as the function pointer type `T`.
///
/// # Safety
///
/// The function there must have type `T`.
unsafe fn cast<T: Copy>(address: NonNull<c_void>) -> T {
	const { assert!(size_of::<T>() == size_of::<NonNull<c_void>>()) };
	// SAFETY: `T` is a function pointer of the address's size, and the
	// caller vouches for its type.
	unsafe { transmute_copy(&address) }
}
