pub mod sapri_sort;
pub mod prove;
pub mod sort_helper;
pub mod utils;

#[cfg(feature = "threadsafe")]
pub use crate::sapri_sort::parallel::sort as sort_interface;

#[cfg(not(feature = "threadsafe"))]
pub use crate::sapri_sort::serial::sort as sort_interface;
