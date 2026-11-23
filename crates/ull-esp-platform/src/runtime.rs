use embassy_executor::Spawner;
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

/// Runs the app: starts logging, calls `app`, and when it fails logs the
/// error and panics, which resets the device with the reason kept.
///
/// ```ignore
/// #[esp_rtos::main]
/// async fn main(spawner: Spawner) {
///     runtime::run_app(spawner, app::run).await
/// }
/// ```
pub async fn run_app<F, Fut, E>(spawner: Spawner, app: F)
where
    F: FnOnce(Spawner) -> Fut,
    Fut: Future<Output = Result<(), E>>,
    E: core::fmt::Display,
{
    init_logger(log::LevelFilter::Info);
    if let Err(err) = app(spawner).await {
        log::error!("{err}");
        panic!("fatal startup error")
    }
}

/// Milliseconds since boot for esp-println's log lines. esp-println looks the
/// function up by name, which takes an unsafe attribute; it only exports the
/// symbol.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "Rust" fn _esp_println_timestamp() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_millis()
}
