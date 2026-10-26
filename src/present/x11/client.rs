//! libxcb and its MIT-SHM extension, opened at run time, with the one
//! function of libX11-xcb that gives an Xlib display's XCB connection:
//! the requests the presenter sends and the replies it polls for.
use crate::library::Library;
use std::{
	ffi::{CStr, c_int, c_uint, c_void},
	mem::transmute_copy,
	ptr::NonNull,
};

/// The libraries' names on Linux and FreeBSD.
pub(crate) const LIBRARIES: [&CStr; 3] = [c"libxcb.so.1", c"libxcb-shm.so.0", c"libX11-xcb.so.1"];

/// An `xcb_connection_t`.
pub(crate) type Connection = *mut c_void;

/// An `xcb_extension_t`, only ever passed by address.
#[repr(C)]
pub(crate) struct Extension {
	_opaque: [u8; 0],
}

/// Any `xcb_*_cookie_t`: the sequence number of the request sent.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cookie {
	pub sequence: c_uint,
}

/// The start of `xcb_query_extension_reply_t`.
#[repr(C)]
pub(crate) struct ExtensionReply {
	pub response_type: u8,
	pub pad: u8,
	pub sequence: u16,
	pub length: u32,
	pub present: u8,
	pub major_opcode: u8,
	pub first_event: u8,
	pub first_error: u8,
}

/// `xcb_shm_query_version_reply_t`.
#[repr(C)]
pub(crate) struct VersionReply {
	pub response_type: u8,
	pub shared_pixmaps: u8,
	pub sequence: u16,
	pub length: u32,
	pub major: u16,
	pub minor: u16,
	pub uid: u16,
	pub gid: u16,
	pub pixmap_format: u8,
	pub pad: [u8; 15],
}

/// `xcb_get_geometry_reply_t`.
#[repr(C)]
pub(crate) struct GeometryReply {
	pub response_type: u8,
	pub depth: u8,
	pub sequence: u16,
	pub length: u32,
	pub root: u32,
	pub x: i16,
	pub y: i16,
	pub width: u16,
	pub height: u16,
	pub border: u16,
	pub pad: [u8; 2],
}

/// An `xcb_generic_error_t`, only ever freed.
pub(crate) type GenericError = c_void;

pub(crate) type XcbConnection = unsafe extern "C" fn(*mut c_void) -> Connection;
pub(crate) type GenerateId = unsafe extern "C" fn(Connection) -> u32;
pub(crate) type Flush = unsafe extern "C" fn(Connection) -> c_int;
pub(crate) type HasError = unsafe extern "C" fn(Connection) -> c_int;
pub(crate) type GetSetup = unsafe extern "C" fn(Connection) -> *const u8;
pub(crate) type MaximumRequestLength = unsafe extern "C" fn(Connection) -> u32;
pub(crate) type ExtensionData =
	unsafe extern "C" fn(Connection, *const Extension) -> *const ExtensionReply;
pub(crate) type QueryVersion = unsafe extern "C" fn(Connection) -> Cookie;
pub(crate) type QueryVersionReply =
	unsafe extern "C" fn(Connection, Cookie, *mut *mut GenericError) -> *mut VersionReply;
pub(crate) type AttachFd = unsafe extern "C" fn(Connection, u32, c_int, u8) -> Cookie;
pub(crate) type Detach = unsafe extern "C" fn(Connection, u32) -> Cookie;
pub(crate) type ShmPutImage = unsafe extern "C" fn(
	Connection,
	u32,
	u32,
	u16,
	u16,
	u16,
	u16,
	u16,
	u16,
	i16,
	i16,
	u8,
	u8,
	u8,
	u32,
	u32,
) -> Cookie;
pub(crate) type PutImage = unsafe extern "C" fn(
	Connection,
	u8,
	u32,
	u32,
	u16,
	u16,
	i16,
	i16,
	u8,
	u8,
	u32,
	*const u8,
) -> Cookie;
pub(crate) type CreateGc = unsafe extern "C" fn(Connection, u32, u32, u32, *const c_void) -> Cookie;
pub(crate) type FreeGc = unsafe extern "C" fn(Connection, u32) -> Cookie;
pub(crate) type GetGeometry = unsafe extern "C" fn(Connection, u32) -> Cookie;
pub(crate) type GetGeometryReply =
	unsafe extern "C" fn(Connection, Cookie, *mut *mut GenericError) -> *mut GeometryReply;
pub(crate) type GetInputFocus = unsafe extern "C" fn(Connection) -> Cookie;
pub(crate) type PollForReply =
	unsafe extern "C" fn(Connection, c_uint, *mut *mut c_void, *mut *mut GenericError) -> c_int;
pub(crate) type DiscardReply = unsafe extern "C" fn(Connection, c_uint);
pub(crate) type RequestCheck = unsafe extern "C" fn(Connection, Cookie) -> *mut GenericError;

pub(crate) struct Client {
	/// `XGetXCBConnection`, from libX11-xcb.
	pub connection: XcbConnection,
	pub generate_id: GenerateId,
	pub flush: Flush,
	pub has_error: HasError,
	pub setup: GetSetup,
	pub maximum_request_length: MaximumRequestLength,
	pub extension_data: ExtensionData,
	pub shm: *const Extension,
	pub query_version: QueryVersion,
	pub query_version_reply: QueryVersionReply,
	pub attach_fd: AttachFd,
	pub detach: Detach,
	pub shm_put_image: ShmPutImage,
	pub put_image: PutImage,
	pub create_gc: CreateGc,
	pub free_gc: FreeGc,
	pub geometry: GetGeometry,
	pub geometry_reply: GetGeometryReply,
	pub input_focus: GetInputFocus,
	pub poll_for_reply: PollForReply,
	pub discard_reply: DiscardReply,
	pub request_check: RequestCheck,
	/// Kept open while their functions are in use.
	_libraries: Vec<Library>,
}

impl Client {
	/// The system's libxcb, libxcb-shm and libX11-xcb.
	pub(crate) fn load() -> Result<Self, String> {
		let libraries = LIBRARIES
			.into_iter()
			.map(Library::open)
			.collect::<Result<Vec<_>, _>>()?;
		let lookup = |name: &CStr| libraries.iter().find_map(|library| library.symbol(name));
		let client = Self::find(lookup)?;
		Ok(Self {
			_libraries: libraries,
			..client
		})
	}

	/// The client whose symbols `lookup` finds, or the first it lacks.
	pub(crate) fn find(lookup: impl Fn(&CStr) -> Option<NonNull<c_void>>) -> Result<Self, String> {
		let function =
			|name: &CStr| lookup(name).ok_or_else(|| format!("no {} in libxcb", name.to_string_lossy()));
		// SAFETY: the libraries export each function with the type of the
		// field it goes to, as xcb.h, xproto.h, shm.h and Xlib-xcb.h declare
		// it, and `xcb_shm_id` as an `xcb_extension_t`.
		unsafe {
			Ok(Self {
				connection: cast(function(c"XGetXCBConnection")?),
				generate_id: cast(function(c"xcb_generate_id")?),
				flush: cast(function(c"xcb_flush")?),
				has_error: cast(function(c"xcb_connection_has_error")?),
				setup: cast(function(c"xcb_get_setup")?),
				maximum_request_length: cast(function(c"xcb_get_maximum_request_length")?),
				extension_data: cast(function(c"xcb_get_extension_data")?),
				shm: function(c"xcb_shm_id")?.as_ptr().cast_const().cast(),
				query_version: cast(function(c"xcb_shm_query_version")?),
				query_version_reply: cast(function(c"xcb_shm_query_version_reply")?),
				attach_fd: cast(function(c"xcb_shm_attach_fd_checked")?),
				detach: cast(function(c"xcb_shm_detach")?),
				shm_put_image: cast(function(c"xcb_shm_put_image")?),
				put_image: cast(function(c"xcb_put_image")?),
				create_gc: cast(function(c"xcb_create_gc_checked")?),
				free_gc: cast(function(c"xcb_free_gc")?),
				geometry: cast(function(c"xcb_get_geometry")?),
				geometry_reply: cast(function(c"xcb_get_geometry_reply")?),
				input_focus: cast(function(c"xcb_get_input_focus")?),
				poll_for_reply: cast(function(c"xcb_poll_for_reply")?),
				discard_reply: cast(function(c"xcb_discard_reply")?),
				request_check: cast(function(c"xcb_request_check")?),
				_libraries: Vec::new(),
			})
		}
	}
}

/// The function at `address`, as type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`, a function pointer.
unsafe fn cast<F: Copy>(address: NonNull<c_void>) -> F {
	const { assert!(size_of::<F>() == size_of::<NonNull<c_void>>()) };
	// SAFETY: as the caller vouches; the sizes match.
	unsafe { transmute_copy(&address) }
}
