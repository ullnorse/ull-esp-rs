#![no_std]

extern crate alloc;
pub extern crate esp_alloc as __esp_alloc;

#[macro_export]
macro_rules! define_heap {
    ($vis:vis $name:ident, $size:expr) => {
        $vis fn $name() {
            static mut HEAP: core::mem::MaybeUninit<[u8; $size]> = core::mem::MaybeUninit::uninit();

            unsafe {
                $crate::__esp_alloc::HEAP.add_region($crate::__esp_alloc::HeapRegion::new(
                    HEAP.as_mut_ptr() as *mut u8,
                    $size,
                    $crate::__esp_alloc::MemoryCapability::Internal.into(),
                ));
            }
        }
    };
}

pub mod config;
pub mod error;
pub mod flash;
pub mod i2c;
pub mod ota;
pub mod runtime;
pub mod spi;
pub mod uart;
pub mod wifi;
