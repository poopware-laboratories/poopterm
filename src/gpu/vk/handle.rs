//! Vulkan handles, each a type of its own laid out as the C handle it
//! wraps, so passing an image where the driver wants a buffer, or swapping
//! two handles of different kinds, does not compile.
use std::{ffi::c_void, fmt, ptr};

/// Handles the specification declares with `VK_DEFINE_NON_DISPATCHABLE_HANDLE`:
/// 64 bits on every platform, zero for `VK_NULL_HANDLE`. They print as the
/// number they hold, so messages read as the driver's handles do.
macro_rules! non_dispatchable {
	($($(#[$doc:meta])* $name:ident;)*) => {$(
		$(#[$doc])*
		#[repr(transparent)]
		#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
		pub(crate) struct $name(pub(crate) u64);

		impl $name {
			/// `VK_NULL_HANDLE`.
			pub(crate) const NULL: Self = Self(0);
		}

		impl fmt::Debug for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				fmt::Debug::fmt(&self.0, f)
			}
		}

		impl fmt::LowerHex for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				fmt::LowerHex::fmt(&self.0, f)
			}
		}
	)*};
}

/// Handles the specification declares with `VK_DEFINE_HANDLE`: pointers to
/// the driver's objects, null for `NULL`, printed as the pointer.
macro_rules! dispatchable {
	($($(#[$doc:meta])* $name:ident;)*) => {$(
		$(#[$doc])*
		#[repr(transparent)]
		#[derive(Clone, Copy, PartialEq, Eq, Hash)]
		pub(crate) struct $name(pub(crate) *mut c_void);

		impl $name {
			/// `NULL`.
			pub(crate) const NULL: Self = Self(ptr::null_mut());
		}

		impl Default for $name {
			fn default() -> Self {
				Self::NULL
			}
		}

		impl fmt::Debug for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				fmt::Debug::fmt(&self.0, f)
			}
		}
	)*};
}

dispatchable! {
	/// A `VkInstance`.
	RawInstance;
	/// A `VkPhysicalDevice`.
	RawPhysicalDevice;
	/// A `VkDevice`.
	RawDevice;
	/// A `VkQueue`.
	RawQueue;
	/// A `VkCommandBuffer`.
	RawCommandBuffer;
}

non_dispatchable! {
	/// A `VkBuffer`.
	RawBuffer;
	/// A `VkDeviceMemory`.
	RawMemory;
	/// A `VkImage`.
	RawImage;
	/// A `VkImageView`.
	RawImageView;
	/// A `VkSemaphore`.
	RawSemaphore;
	/// A `VkCommandPool`.
	RawCommandPool;
	/// A `VkShaderModule`.
	RawShaderModule;
	/// A `VkSampler`.
	RawSampler;
	/// A `VkDescriptorSetLayout`.
	RawDescriptorSetLayout;
	/// A `VkPipelineLayout`.
	RawPipelineLayout;
	/// A `VkPipeline`.
	RawPipeline;
	/// A `VkDescriptorPool`.
	RawDescriptorPool;
	/// A `VkDescriptorSet`.
	RawDescriptorSet;
	/// A `VkSurfaceKHR`.
	RawSurface;
	/// A `VkSwapchainKHR`.
	RawSwapchain;
	/// A `VkQueryPool`.
	RawQueryPool;
}
