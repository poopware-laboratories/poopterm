//! Picks a wallpaper from Photos and copies it into the app's private data directory.
// Calls into PhotosUI, Foundation and the Objective-C runtime; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::{
	ffi::{CStr, c_char, c_void},
	fs,
	path::PathBuf,
	sync::{
		Mutex, OnceLock,
		atomic::{AtomicUsize, Ordering},
	},
};

use winit::{
	event_loop::EventLoopProxy,
	raw_window_handle::{HasWindowHandle, RawWindowHandle},
	window::Window,
};

use crate::{
	objc::{self, Arguments, Object, Raw, Sel},
	pty::UserEvent,
};

const DELEGATE: &CStr = c"PooptermPhotoPickerDelegate";
const BLOCK_IS_GLOBAL: i32 = 1 << 28;
static DELEGATE_OBJECT: AtomicUsize = AtomicUsize::new(0);
static PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);

#[link(name = "System")]
unsafe extern "C" {
	static _NSConcreteGlobalBlock: [*mut c_void; 32];
}

#[repr(C)]
struct BlockDescriptor {
	reserved: usize,
	size: usize,
}

#[repr(C)]
struct FileBlock {
	isa: usize,
	flags: i32,
	reserved: i32,
	invoke: extern "C" fn(*mut FileBlock, Raw, Raw),
	descriptor: usize,
}

static FILE_BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
	reserved: 0,
	size: std::mem::size_of::<FileBlock>(),
};
static FILE_BLOCK: OnceLock<FileBlock> = OnceLock::new();

/// The capture-free block used by NSItemProvider for its asynchronous file copy.
fn file_block() -> *const c_void {
	let block = FILE_BLOCK.get_or_init(|| {
		// The Blocks runtime exports the global-block class object; the literal has the documented
		// layout, a matching descriptor size, and captures no Rust data.
		let isa = std::ptr::addr_of!(_NSConcreteGlobalBlock) as usize;
		FileBlock {
			isa,
			flags: BLOCK_IS_GLOBAL,
			reserved: 0,
			invoke: file_loaded,
			descriptor: std::ptr::addr_of!(FILE_BLOCK_DESCRIPTOR) as usize,
		}
	});
	std::ptr::from_ref(block).cast()
}

/// Registers the Photos picker delegate before app windows appear.
pub(crate) fn register(proxy: EventLoopProxy<UserEvent>) {
	*PROXY.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(proxy);
	let methods = [objc::Method {
		selector: c"picker:didFinishPicking:",
		imp: picked as *const c_void,
		types: c"v@:@@",
	}];
	// SAFETY: the callback signature matches the PHPickerViewControllerDelegate encoding.
	let Ok(class) = (unsafe {
		objc::subclass(
			DELEGATE,
			c"NSObject",
			c"PHPickerViewControllerDelegate",
			&methods,
		)
	}) else {
		crate::warnings::warning!("could not register Photos picker callbacks");
		return;
	};
	// SAFETY: the delegate class descends from NSObject and has a no-argument initializer.
	if let Some(object) = unsafe { class.instance() } {
		let raw = object.raw() as usize;
		std::mem::forget(object);
		DELEGATE_OBJECT.store(raw, Ordering::Release);
	}
}

/// Presents the native Photos gallery from the live Settings window.
pub(crate) fn choose(window: &Window) -> Result<(), String> {
	let handle = window.window_handle().map_err(|error| error.to_string())?;
	let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
		return Err("Settings has no UIKit window".into());
	};
	// SAFETY: the raw handle belongs to the live winit UIView; UIKit returns its window or nil.
	let ui_window: Raw =
		unsafe { objc::Arguments::send((), handle.ui_view.as_ptr(), objc::sel(c"window")) };
	if ui_window.is_null() {
		return Err("Settings is not attached to a window".into());
	}
	// SAFETY: rootViewController returns the live window's retained controller.
	let root: Raw = unsafe { objc::Arguments::send((), ui_window, objc::sel(c"rootViewController")) };
	// SAFETY: retaining the non-nil root keeps it alive through presentation.
	let root = unsafe { Object::retained(root) }.ok_or("Settings has no view controller")?;
	if !root.responds(c"presentedViewController")
		|| !root.responds(c"presentViewController:animated:completion:")
	{
		return Err("Settings cannot present Photos right now".into());
	}
	let mut presenter = root;
	for depth in 0..32 {
		// SAFETY: UIViewController's presentedViewController returns nil or its live presented child.
		let presented: Raw = unsafe { presenter.send(c"presentedViewController", ()) };
		// SAFETY: retaining nil is safe; a non-nil child is a live NSObject returned by UIKit.
		let Some(presented) = (unsafe { Object::retained(presented) }) else {
			break;
		};
		if depth == 31 {
			return Err("Settings has too many presented screens".into());
		}
		presenter = presented;
	}
	let filter_class = objc::class(c"PHPickerFilter").map_err(|error| error.to_string())?;
	// SAFETY: imagesFilter is a no-argument class property returning the Photos filter.
	let filter: Raw = unsafe { filter_class.send(c"imagesFilter", ()) };
	if filter.is_null() {
		return Err("Photos could not create the image filter".into());
	}
	let configuration_class =
		objc::class(c"PHPickerConfiguration").map_err(|error| error.to_string())?;
	// SAFETY: alloc returns an owned PHPickerConfiguration allocation.
	let allocated: Raw = unsafe { configuration_class.send(c"alloc", ()) };
	// SAFETY: PHPickerConfiguration's init takes no arguments.
	let configuration_raw: Raw = unsafe { Arguments::send((), allocated, objc::sel(c"init")) };
	// SAFETY: init returns the owned configuration; nil means Photos could not create it.
	let configuration =
		unsafe { Object::owned(configuration_raw) }.ok_or("Photos could not open the gallery")?;
	// SAFETY: selectionLimit is NSInteger; the configuration is live and mutable here.
	unsafe { configuration.send::<()>(c"setSelectionLimit:", (1isize,)) };
	// SAFETY: setFilter: copies the live PHPickerFilter object.
	unsafe { configuration.send::<()>(c"setFilter:", (filter,)) };
	// Prefer a compatible representation so HEIC photos arrive in a format our image loader reads.
	// SAFETY: the property is an NSInteger enum and value 2 is Compatible (iOS 14+).
	unsafe { configuration.send::<()>(c"setPreferredAssetRepresentationMode:", (2isize,)) };
	let picker_class = objc::class(c"PHPickerViewController").map_err(|error| error.to_string())?;
	// SAFETY: alloc returns an owned PHPickerViewController allocation.
	let allocated: Raw = unsafe { picker_class.send(c"alloc", ()) };
	// SAFETY: initWithConfiguration: takes a live PHPickerConfiguration object.
	let picker_raw: Raw =
		unsafe { (configuration.raw(),).send(allocated, objc::sel(c"initWithConfiguration:")) };
	// SAFETY: init returns the owned picker; nil means Photos could not create it.
	let picker = unsafe { Object::owned(picker_raw) }.ok_or("Photos could not open the gallery")?;
	let delegate = DELEGATE_OBJECT.load(Ordering::Acquire) as Raw;
	if delegate.is_null() {
		return Err("Photos picker is unavailable".into());
	}
	// SAFETY: PHPicker's delegate is weak; this delegate is retained for the app's lifetime.
	unsafe { picker.send::<()>(c"setDelegate:", (delegate,)) };
	// SAFETY: UIKit presents the live picker from the topmost Settings controller on the main thread.
	unsafe {
		presenter.send::<()>(
			c"presentViewController:animated:completion:",
			(picker.raw(), 1i8, objc::NIL),
		)
	};
	Ok(())
}

/// UIKit calls this for a selection or for Cancel; the picker remains presented until dismissed.
extern "C" fn picked(_: Raw, _: Sel, picker: Raw, results: Raw) {
	// SAFETY: dismissViewControllerAnimated:completion: takes BOOL and a nullable block.
	unsafe {
		Arguments::send::<()>(
			(1i8, objc::NIL),
			picker,
			objc::sel(c"dismissViewControllerAnimated:completion:"),
		)
	};
	// SAFETY: retaining nil is safe; a non-nil array is live during this delegate callback.
	let Some(results) = (unsafe { Object::retained(results) }) else {
		send_event(UserEvent::WallpaperPickerCancelled);
		return;
	};
	// SAFETY: firstObject returns nil or the live result retained by the array.
	let first: Raw = unsafe { results.send(c"firstObject", ()) };
	// SAFETY: retaining nil is safe; a non-nil result is held by the retained array.
	let Some(first) = (unsafe { Object::retained(first) }) else {
		send_event(UserEvent::WallpaperPickerCancelled);
		return;
	};
	// SAFETY: PHPickerResult.itemProvider returns a provider retained by the result.
	let raw: Raw = unsafe { first.send(c"itemProvider", ()) };
	// SAFETY: retaining nil is safe; the result owns any non-nil provider.
	let Some(provider) = (unsafe { Object::retained(raw) }) else {
		send_event(UserEvent::WallpaperImported(Err(
			"Photos did not provide the selected image".into(),
		)));
		return;
	};
	let Some(image_type) = objc::string("public.image") else {
		send_event(UserEvent::WallpaperImported(Err(
			"could not prepare the Photos image request".into(),
		)));
		return;
	};
	// SAFETY: loadFileRepresentation takes an NSString and a global block using Apple's block ABI.
	unsafe {
		provider.send::<Raw>(
			c"loadFileRepresentationForTypeIdentifier:completionHandler:",
			(image_type.raw(), file_block()),
		)
	};
}

/// The capture-free completion block copies the temporary image before Photos removes it.
extern "C" fn file_loaded(_: *mut FileBlock, url: Raw, error: Raw) {
	let _pool = objc::Pool::new();
	// SAFETY: retaining nil is safe; a non-nil NSURL remains valid through this completion callback.
	let result = match unsafe { Object::retained(url) } {
		Some(url) => copy_url(&url),
		None => {
			// SAFETY: the provider supplies nil or an NSError for a failed load.
			Err(unsafe { objc::describe(error, "Photos could not load that image") })
		}
	};
	send_event(UserEvent::WallpaperImported(result));
}

fn send_event(event: UserEvent) {
	let proxy = PROXY.lock().unwrap_or_else(|poison| poison.into_inner());
	if let Some(proxy) = proxy.as_ref() {
		let _ = proxy.send_event(event);
	}
}

/// Copies Photos' temporary file into the app's private data directory.
fn copy_url(url: &Object) -> Result<PathBuf, String> {
	// SAFETY: NSURL's `path` returns its NSString path, retained for this conversion.
	let raw: Raw = unsafe { url.send(c"path", ()) };
	// SAFETY: the NSString returned by path is live while the retained NSURL is.
	let path = unsafe { Object::retained(raw) }.ok_or("selected photo has no path")?;
	// SAFETY: NSString returns a NUL-terminated UTF-8 pointer valid while `path` lives.
	let bytes: *const c_char = unsafe { path.send(c"UTF8String", ()) };
	if bytes.is_null() {
		return Err("selected photo path is not UTF-8".into());
	}
	// SAFETY: bytes is NSString's valid NUL-terminated UTF-8 representation.
	let source = unsafe { CStr::from_ptr(bytes) }
		.to_str()
		.map_err(|error| error.to_string())?;
	let source = PathBuf::from(source);
	let name = source.file_name().ok_or("selected photo has no name")?;
	let size = fs::metadata(&source)
		.map_err(|error| format!("could not read selected photo: {error}"))?
		.len();
	if size > 128 * 1024 * 1024 {
		return Err("Wallpaper image must be smaller than 128 MiB.".into());
	}
	let destination = crate::config::data_dir().ok_or("app data directory is unavailable")?;
	fs::create_dir_all(&destination)
		.map_err(|error| format!("could not create {}: {error}", destination.display()))?;
	let ticks = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_err(|error| error.to_string())?
		.as_micros();
	let target = destination.join(format!("wallpaper-{ticks}"));
	fs::copy(&source, &target).map_err(|error| format!("{}: {error}", name.to_string_lossy()))?;
	Ok(target)
}
