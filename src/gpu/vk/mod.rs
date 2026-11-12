//! Our own Vulkan bindings: the loader opened at run time, so Poopterm
//! runs where there is no Vulkan, and only the types and functions the
//! renderer uses, laid out as the specification declares them.
pub(crate) mod allocation;
#[cfg(test)]
mod allocation_tests;
pub(crate) mod buffer;
#[cfg(test)]
mod buffer_tests;
pub(crate) mod call;
#[cfg(test)]
mod call_tests;
pub(crate) mod choose;
#[cfg(test)]
mod choose_reason_tests;
#[cfg(test)]
mod choose_tests;
pub(crate) mod commands;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod descriptor_driver_tests;
pub(crate) mod descriptor_types;
#[cfg(test)]
mod descriptor_types_tests;
pub(crate) mod descriptors;
#[cfg(test)]
mod descriptors_tests;
pub(crate) mod device;
#[cfg(test)]
mod device_optional_tests;
#[cfg(test)]
mod device_tests;
pub(crate) mod draw;
#[cfg(test)]
mod draw_driver_tests;
#[cfg(test)]
mod draw_gpu_tests;
#[cfg(test)]
mod draw_rules_tests;
#[cfg(test)]
mod draw_runs_tests;
#[cfg(test)]
mod draw_tests;
pub(crate) mod draw_types;
#[cfg(test)]
mod draw_types_tests;
pub(crate) mod driver_info;
#[cfg(test)]
mod driver_info_tests;
#[cfg(test)]
mod driver_tests;
pub(crate) mod entry;
pub(crate) mod fault;
#[cfg(test)]
mod fault_tests;
pub(crate) mod features;
#[cfg(test)]
mod features_tests;
pub(crate) mod image;
#[cfg(test)]
mod image_tests;
pub(crate) mod instance;
#[cfg(test)]
mod loader_tests;
pub(crate) mod memory;
pub(crate) mod memory_budget;
#[cfg(test)]
mod memory_budget_tests;
#[cfg(test)]
mod memory_driver_tests;
#[cfg(test)]
mod memory_tests;
pub(crate) mod optional;
#[cfg(test)]
mod optional_tests;
#[cfg(test)]
pub(crate) mod outside_tests;
pub(crate) mod pipeline;
#[cfg(test)]
mod pipeline_driver_tests;
#[cfg(test)]
mod pipeline_tests;
pub(crate) mod pipeline_types;
#[cfg(test)]
mod pipeline_types_tests;
pub(crate) mod present;
#[cfg(test)]
mod present_driver_tests;
pub(crate) mod present_regions;
#[cfg(test)]
mod present_regions_tests;
#[cfg(test)]
mod present_tests;
pub(crate) mod recording;
#[cfg(test)]
mod recording_tests;
pub(crate) mod renderer;
#[cfg(test)]
mod renderer_gpu_tests;
#[cfg(test)]
mod renderer_image_edge_tests;
#[cfg(test)]
mod renderer_image_tests;
#[cfg(test)]
mod renderer_shrink_gpu_tests;
#[cfg(test)]
mod renderer_shrink_tests;
#[cfg(test)]
mod renderer_tests;
#[cfg(test)]
mod round_trip_tests;
pub(crate) mod semaphore;
pub(crate) mod shader;
#[cfg(test)]
mod shader_tests;
pub(crate) mod stamps;
#[cfg(test)]
mod stamps_tests;
pub(crate) mod submit_types;
#[cfg(test)]
mod submit_types_tests;
pub(crate) mod surface;
#[cfg(test)]
mod surface_driver_tests;
#[cfg(test)]
mod surface_query_tests;
pub(crate) mod surface_types;
#[cfg(test)]
mod surface_types_tests;
pub(crate) mod swapchain;
#[cfg(test)]
mod swapchain_driver_tests;
pub(crate) mod swapchain_plan;
#[cfg(test)]
mod swapchain_plan_tests;
#[cfg(test)]
mod swapchain_tests;
pub(crate) mod swapchain_types;
#[cfg(test)]
mod swapchain_types_tests;
#[cfg(test)]
mod texture_set_tests;
pub(crate) mod timeline;
#[cfg(test)]
mod timeline_tests;
pub(crate) mod types;
#[cfg(test)]
mod types_tests;
pub(crate) mod version;
pub(crate) mod window;
#[cfg(test)]
mod window_atlas_tests;
#[cfg(test)]
mod window_budget_tests;
#[cfg(test)]
mod window_facts_tests;
#[cfg(test)]
mod window_frame_tests;
#[cfg(test)]
mod window_gpu_tests;
#[cfg(test)]
mod window_latest_ready_tests;
#[cfg(test)]
mod window_open_tests;
#[cfg(test)]
mod window_regions_tests;
#[cfg(test)]
mod window_tests;
