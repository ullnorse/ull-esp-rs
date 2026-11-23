//! Restarts the device when it stops making progress.
//!
//! Two layers:
//!
//! - A [`Pulse`] per task that matters. The task promises its next beat
//!   within a deadline; a watchdog task checks the pulses every second and,
//!   when one is late, reboots with a [`NoteKind::Stall`] note naming it.
//! - The TIMG1 hardware watchdog, fed by that same task. If the executor
//!   itself stops (a task that never yields, an interrupt storm), nothing
//!   feeds it and the chip resets after [`HARDWARE_TIMEOUT`].
//!
//! ```ignore
//! watchdog::start(spawner, peripherals.TIMG1, &[&fleet::PULSE])?;
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::peripherals::TIMG1;
use esp_hal::timer::timg::{MwdtStage, TimerGroup, Wdt};

use crate::error::EspError;
use crate::restart::{self, NoteKind};

/// The hardware watchdog resets the chip when not fed for this long.
pub const HARDWARE_TIMEOUT: Duration = Duration::from_secs(10);
const CHECK_PERIOD: Duration = Duration::from_secs(1);

/// A task's promise to report progress. Starts disarmed: a pulse is checked
/// only after its first beat.
pub struct Pulse {
    name: &'static str,
    armed: AtomicBool,
    /// Milliseconds since boot, wrapping.
    deadline_ms: AtomicU32,
}

impl Pulse {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            armed: AtomicBool::new(false),
            deadline_ms: AtomicU32::new(0),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Promises the next beat within `within`. Keep it under 24 days.
    pub fn beat_within(&self, within: Duration) {
        let deadline = now_ms().wrapping_add(within.as_millis() as u32);
        self.deadline_ms.store(deadline, Ordering::Relaxed);
        self.armed.store(true, Ordering::Release);
    }

    /// Stops checking this pulse until its next beat.
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }

    fn is_late(&self, now: u32) -> bool {
        self.armed.load(Ordering::Acquire)
            && (now.wrapping_sub(self.deadline_ms.load(Ordering::Relaxed)) as i32) > 0
    }
}

fn now_ms() -> u32 {
    Instant::now().as_millis() as u32
}

/// Starts the watchdog task and the hardware watchdog.
pub fn start(
    spawner: Spawner,
    timg1: TIMG1<'static>,
    pulses: &'static [&'static Pulse],
) -> Result<(), EspError> {
    // Read the last restart before anything can leave a new note.
    let _ = restart::last();
    let mut wdt = TimerGroup::new(timg1).wdt;
    wdt.set_timeout(
        MwdtStage::Stage0,
        esp_hal::time::Duration::from_millis(HARDWARE_TIMEOUT.as_millis()),
    );
    wdt.enable();
    wdt.feed();
    spawner.spawn(watchdog_task(wdt, pulses).map_err(|_| EspError::TaskSpawn("watchdog"))?);
    Ok(())
}

#[embassy_executor::task]
async fn watchdog_task(mut wdt: Wdt<TIMG1<'static>>, pulses: &'static [&'static Pulse]) {
    loop {
        let now = now_ms();
        if let Some(late) = pulses.iter().find(|pulse| pulse.is_late(now)) {
            log::error!("watchdog: {} stopped reporting progress", late.name);
            restart::reboot(NoteKind::Stall, late.name);
        }
        wdt.feed();
        Timer::after(CHECK_PERIOD).await;
    }
}
