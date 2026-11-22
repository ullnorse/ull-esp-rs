use esp_hal::clock::CpuClock;
use esp_hal::peripherals::{FROM_CPU_INTR0, TIMG0};
use esp_hal::timer::timg::TimerGroup;

pub fn max_clock_config() -> esp_hal::Config {
    esp_hal::Config::default().with_cpu_clock(CpuClock::max())
}

pub fn init_logger(level: log::LevelFilter) {
    esp_println::logger::init_logger(level);
}

pub fn init_default_heap() {
    esp_alloc::heap_allocator!(size: 64 * 1024);
}

pub fn start(timg0: TIMG0<'static>, int0: FROM_CPU_INTR0<'static>) {
    let timg0 = TimerGroup::new(timg0);
    esp_rtos::start(timg0.timer0, int0);
}
