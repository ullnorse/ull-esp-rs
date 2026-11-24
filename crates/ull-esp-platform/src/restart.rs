//! Restart reasons that survive the reset, and the panic handler that leaves
//! one.
//!
//! Reset on purpose with [`reboot`] instead of `software_reset`, so the next
//! boot knows why. The crate's panic handler logs the panic, keeps its
//! location and message, and resets the device instead of hanging. The next
//! boot reads the reason with [`last`] and the fleet agent reports it.

use core::cell::RefCell;
use core::fmt::Write as _;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_hal::rtc_cntl::SocResetReason;
use portable_atomic::{AtomicU32, Ordering};
use ull_esp_core::restart::{self as core_restart, ChipReset, DetailWriter, NOTE_WORDS};

pub use ull_esp_core::restart::{Cause, NoteKind, Restart};

/// Survives software and watchdog resets; zeroed on power-on.
#[esp_hal::ram(unstable(rtc_fast, persistent))]
static NOTE: [AtomicU32; NOTE_WORDS] = [const { AtomicU32::new(0) }; NOTE_WORDS];

static LAST: Mutex<CriticalSectionRawMutex, RefCell<Option<Restart>>> =
    Mutex::new(RefCell::new(None));

/// How long a panic waits before the reset, so its log line gets out.
const PANIC_RESET_DELAY_MS: u64 = 1000;

/// Why this boot happened. The first call reads and clears the note, so a
/// later reset that leaves none is not blamed on an old one; call it early
/// (the board and the fleet agent do).
pub fn last() -> Restart {
    LAST.lock(|last| {
        last.borrow_mut()
            .get_or_insert_with(|| {
                let words = read_note();
                write_note(&[0; NOTE_WORDS]);
                Restart::new(chip_reset(), core_restart::decode(&words))
            })
            .clone()
    })
}

/// Leaves a note and resets the device.
pub fn reboot(kind: NoteKind, detail: &str) -> ! {
    // Read the previous note first, or this one would be cleared by it.
    let _ = last();
    note_and_reset(kind, detail)
}

/// Without touching [`LAST`], which a panic may have left borrowed.
fn note_and_reset(kind: NoteKind, detail: &str) -> ! {
    let mut words = [0u32; NOTE_WORDS];
    core_restart::encode(kind, detail, &mut words);
    write_note(&words);
    esp_hal::system::software_reset()
}

/// The panic handler: log, keep the message for the next boot, reset.
/// Every firmware that links this crate gets it.
#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo<'_>) -> ! {
    log::error!("PANIC: {info}");
    let mut detail = DetailWriter::new();
    if let Some(location) = info.location() {
        let _ = write!(
            detail,
            "{}:{}: ",
            core_restart::short_path(location.file()),
            location.line()
        );
    }
    let _ = write!(detail, "{}", info.message());
    let start = esp_hal::time::Instant::now();
    while start.elapsed().as_millis() < PANIC_RESET_DELAY_MS {}
    note_and_reset(NoteKind::Panic, detail.as_str())
}

fn read_note() -> [u32; NOTE_WORDS] {
    core::array::from_fn(|index| NOTE[index].load(Ordering::SeqCst))
}

fn write_note(words: &[u32; NOTE_WORDS]) {
    for (slot, word) in NOTE.iter().zip(words) {
        slot.store(*word, Ordering::SeqCst);
    }
}

fn chip_reset() -> ChipReset {
    let reason = esp_hal::system::reset_reason();
    if reason.is_some_and(is_cpu_watchdog) {
        return ChipReset::Watchdog;
    }
    match reason {
        Some(SocResetReason::ChipPowerOn) => ChipReset::PowerOn,
        Some(SocResetReason::CoreSw | SocResetReason::Cpu0Sw) => ChipReset::Software,
        Some(
            SocResetReason::CoreMwdt0
            | SocResetReason::CoreMwdt1
            | SocResetReason::CoreRtcWdt
            | SocResetReason::Cpu0RtcWdt
            | SocResetReason::SysRtcWdt,
        ) => ChipReset::Watchdog,
        Some(SocResetReason::SysBrownOut) => ChipReset::Brownout,
        Some(SocResetReason::CoreDeepSleep) => ChipReset::DeepSleep,
        _ => ChipReset::Other,
    }
}

/// The CPU watchdog reset variants differ between chips.
#[cfg(feature = "esp32")]
fn is_cpu_watchdog(reason: SocResetReason) -> bool {
    matches!(reason, SocResetReason::CpuMwdt0)
}

#[cfg(not(feature = "esp32"))]
fn is_cpu_watchdog(reason: SocResetReason) -> bool {
    matches!(
        reason,
        SocResetReason::Cpu0Mwdt0 | SocResetReason::Cpu0Mwdt1 | SocResetReason::SysSuperWdt
    )
}
