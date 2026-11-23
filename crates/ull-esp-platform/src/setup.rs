//! Setup mode: the board opens its own Wi-Fi network with a page where a
//! phone enters the Wi-Fi name, password and fleet server.
//!
//! The board enters setup mode when it has no Wi-Fi settings, when the BOOT
//! button is held for 3 seconds ([`button_task`]), or when the Wi-Fi has been
//! down for 5 minutes ([`fallback_task`]). The last two reboot with a
//! [`NoteKind::Setup`] note, and [`reason`] finds it on the next boot.
//!
//! Setup mode leaves by rebooting: after a save, or after 10 minutes without
//! a request, so a board whose router was only off for a while goes back to
//! trying it.

extern crate alloc;

use alloc::string::String;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{
    Config as NetConfig, Ipv4Address, Ipv4Cidr, Stack, StackResources, StaticConfigV4,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Write as _;
use esp_hal::gpio::Input;
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::ap::AccessPointConfig;
use esp_radio::wifi::scan::{ScanConfig, ScanTypeConfig};
use esp_radio::wifi::{
    AuthenticationMethod, Config as WifiConfiguration, Interface, Ssid, WifiController,
};
use esp_storage::FlashStorage;
use heapless::Vec;
use static_cell::StaticCell;
use ull_esp_core::fleet::DeviceId;
use ull_esp_core::http::parse_request_head;
use ull_esp_core::settings::Text;
use ull_esp_core::setup::page::{self, FormView, Network};
use ull_esp_core::setup::{dhcp, dns, form};

use crate::error::EspError;
use crate::restart::{self, Cause, NoteKind};
use crate::settings::{self, Settings, Source};
use crate::wifi::{WifiRunner, runner_task};

/// The board's address on its setup network.
pub const SETUP_IP: Ipv4Address = Ipv4Address::new(192, 168, 4, 1);
/// Setup mode reboots after this long without a request.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// [`fallback_task`] enters setup mode after the Wi-Fi is down this long.
pub const WIFI_DOWN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const BUTTON_HOLD: Duration = Duration::from_secs(3);
const MAX_NETWORKS: usize = 12;
const HTTP_WORKERS: usize = 4;
/// DHCP, DNS and the HTTP workers.
const SOCKETS: usize = 2 + HTTP_WORKERS;
const REQUEST_MAX: usize = 1024;

/// Why setup mode should run on this boot, or `None` to start normally.
pub fn reason(source: Source) -> Option<&'static str> {
    let last = restart::last();
    if last.cause == Cause::Setup {
        return Some(match last.detail() {
            Some(_) => "asked for on the last boot",
            None => "asked for",
        });
    }
    (source == Source::None).then_some("no Wi-Fi settings")
}

/// Name of the setup network: `ull-setup-` and the last four hex digits of
/// the MAC address, as printed on the device page.
pub fn network_name() -> Text<32> {
    let mac = esp_hal::efuse::base_mac_address();
    let bytes = mac.as_bytes();
    let mut name = Text::empty();
    let _ = write!(name, "ull-setup-{:02x}{:02x}", bytes[4], bytes[5]);
    name
}

/// Runs setup mode until it reboots.
pub async fn run(
    spawner: Spawner,
    wifi: WIFI<'static>,
    flash: FlashStorage<'static>,
    current: &'static Settings,
    firmware: &'static str,
    seed: u64,
) -> ! {
    static PAGE: StaticCell<Page> = StaticCell::new();

    // `_controller` stays alive here: dropping it stops the radio.
    let (stack, networks, _controller) = match start(spawner, wifi, seed).await {
        Ok(started) => started,
        Err(err) => {
            log::error!("setup: cannot start the setup network: {err}");
            Timer::after(Duration::from_secs(5)).await;
            restart::reboot(NoteKind::Settings, "setup network failed to start")
        }
    };
    let mac = esp_hal::efuse::base_mac_address();
    let page: &'static Page = PAGE.init(Page {
        device: DeviceId::from_mac(mac.as_bytes().try_into().unwrap_or([0; 6])),
        firmware,
        current,
        networks,
        flash: Mutex::new(flash),
    });
    for _ in 0..HTTP_WORKERS {
        match http_task(stack, page) {
            Ok(task) => spawner.spawn(task),
            Err(_) => log::error!("setup: cannot spawn an http worker"),
        }
    }

    touch();
    loop {
        Timer::after(Duration::from_secs(5)).await;
        let idle = now_ms().wrapping_sub(LAST_REQUEST_MS.load(Ordering::Relaxed));
        if u64::from(idle) >= IDLE_TIMEOUT.as_millis() {
            log::info!("setup: no requests for 10 minutes, restarting");
            restart::reboot(NoteKind::Settings, "setup timed out")
        }
    }
}

/// What every HTTP worker shares.
struct Page {
    device: DeviceId,
    firmware: &'static str,
    current: &'static Settings,
    networks: Vec<Network, MAX_NETWORKS>,
    flash: Mutex<CriticalSectionRawMutex, FlashStorage<'static>>,
}

/// Milliseconds since boot of the last request, wrapping.
static LAST_REQUEST_MS: AtomicU32 = AtomicU32::new(0);

fn now_ms() -> u32 {
    Instant::now().as_millis() as u32
}

fn touch() {
    LAST_REQUEST_MS.store(now_ms(), Ordering::Relaxed);
}

async fn start(
    spawner: Spawner,
    wifi: WIFI<'static>,
    seed: u64,
) -> Result<
    (
        Stack<'static>,
        Vec<Network, MAX_NETWORKS>,
        WifiController<'static>,
    ),
    EspError,
> {
    static RESOURCES: StaticCell<StackResources<SOCKETS>> = StaticCell::new();

    let mut controller = WifiController::new(wifi, Default::default())?;
    // Scanning needs station mode; do it once before opening the network.
    controller.set_config(&WifiConfiguration::Station(Default::default()))?;
    let networks = match controller.scan_async(&scan_config()).await {
        Ok(found) => nearby(
            found
                .iter()
                .map(|ap| (ap.ssid.as_str(), ap.signal_strength, ap.auth_method)),
        ),
        Err(err) => {
            log::warn!("setup: scan failed: {err:?}");
            Vec::new()
        }
    };

    let name = network_name();
    let ssid = Ssid::try_from(name.as_str())?;
    controller.set_config(&WifiConfiguration::AccessPoint(
        AccessPointConfig::default()
            .with_ssid(ssid)
            .with_max_connections(4),
    ))?;

    let (stack, runner): (Stack<'static>, WifiRunner) = embassy_net::new(
        Interface::access_point(),
        NetConfig::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(SETUP_IP, 24),
            gateway: Some(SETUP_IP),
            dns_servers: Default::default(),
        }),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(runner_task(runner).map_err(|_| EspError::TaskSpawn("setup net"))?);
    spawner.spawn(dhcp_task(stack).map_err(|_| EspError::TaskSpawn("setup dhcp"))?);
    spawner.spawn(dns_task(stack).map_err(|_| EspError::TaskSpawn("setup dns"))?);
    log::info!(
        "setup: join the Wi-Fi network \"{}\" and open http://{SETUP_IP}/ ({} networks nearby)",
        name.as_str(),
        networks.len()
    );
    Ok((stack, networks, controller))
}

/// The default dwell (10-20 ms per channel) misses networks; this takes
/// about 4 seconds over 13 channels and finds the weak ones too.
fn scan_config() -> ScanConfig {
    ScanConfig::default().with_scan_type(ScanTypeConfig::Active {
        min: esp_hal::time::Duration::from_millis(120),
        max: esp_hal::time::Duration::from_millis(300),
    })
}

/// Strongest first, one entry per name, hidden networks left out.
fn nearby<'a>(
    found: impl Iterator<Item = (&'a str, i8, Option<AuthenticationMethod>)>,
) -> Vec<Network, MAX_NETWORKS> {
    let mut networks: Vec<Network, 32> = Vec::new();
    for (ssid, rssi, auth) in found {
        let Some(ssid) = Text::new(ssid).filter(|ssid| !ssid.is_empty()) else {
            continue;
        };
        let open = matches!(auth, None | Some(AuthenticationMethod::None));
        match networks.iter_mut().find(|network| network.ssid == ssid) {
            Some(known) if known.rssi < rssi => known.rssi = rssi,
            Some(_) => {}
            None => {
                let _ = networks.push(Network { ssid, rssi, open });
            }
        }
    }
    networks.sort_unstable_by_key(|network| core::cmp::Reverse(network.rssi));
    networks.into_iter().take(MAX_NETWORKS).collect()
}

/// One of [`HTTP_WORKERS`] connections. Phones open several at once, and a
/// connection with no listening socket is refused, so a single worker is not
/// enough. Reboots after a save.
#[embassy_executor::task(pool_size = HTTP_WORKERS)]
async fn http_task(stack: Stack<'static>, page: &'static Page) {
    let mut rx = [0u8; 1536];
    let mut tx = [0u8; 4096];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if let Err(err) = socket.accept(80).await {
            log::warn!("setup: accept failed: {err:?}");
            continue;
        }
        touch();
        serve_one(&mut socket, page).await;
    }
}

async fn serve_one(socket: &mut TcpSocket<'_>, page: &Page) {
    let mut request = [0u8; REQUEST_MAX];
    let Some((head_end, body_len, is_save)) = read_request(socket, &mut request).await else {
        socket.abort();
        return;
    };
    let body = &request[head_end..head_end + body_len];

    let mut html = String::new();
    let mut saved = None;
    let error = if is_save {
        match settings_from_form(body, page.current) {
            Ok(new) => {
                let result = settings::save(&mut *page.flash.lock().await, &new);
                match result {
                    Ok(()) => {
                        let _ = page::render_saved(&mut html, new.wifi_ssid.as_str());
                        saved = Some(new);
                        None
                    }
                    Err(err) => {
                        log::error!("setup: save failed: {err}");
                        Some("Saving failed. Try again.")
                    }
                }
            }
            Err(message) => Some(message),
        }
    } else {
        None
    };
    if saved.is_none() {
        let _ = page::render_form(
            &mut html,
            &FormView {
                device: page.device.as_str(),
                firmware: page.firmware,
                current: page.current,
                networks: &page.networks,
                error,
            },
        );
    }

    let mut head = Text::<160>::empty();
    let _ = write!(
        head,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        html.len()
    );
    let sent = async {
        socket.write_all(head.as_str().as_bytes()).await?;
        socket.write_all(html.as_bytes()).await?;
        socket.flush().await
    }
    .await;
    if let Err(err) = sent {
        log::warn!("setup: reply failed: {err:?}");
    }
    socket.close();
    let _ = with_timeout(Duration::from_millis(300), socket.flush()).await;

    if let Some(new) = saved {
        log::info!(
            "setup: saved settings for \"{}\"; restarting",
            new.wifi_ssid.as_str()
        );
        // Let the phone show the page before the network disappears.
        Timer::after(Duration::from_secs(2)).await;
        restart::reboot(NoteKind::Settings, "new settings saved");
    }
}

/// Reads one request into `buf`; returns where the body starts, its length,
/// and whether it is the form's `POST /save`.
async fn read_request(
    socket: &mut TcpSocket<'_>,
    buf: &mut [u8; REQUEST_MAX],
) -> Option<(usize, usize, bool)> {
    let mut len = 0;
    loop {
        if len == buf.len() {
            return None;
        }
        let read = socket.read(&mut buf[len..]).await.ok()?;
        if read == 0 {
            return None;
        }
        len += read;
        let Some(head) = parse_request_head(&buf[..len]).ok()? else {
            continue;
        };
        let is_save = head.method == "POST" && head.path == "/save";
        let body_len = if is_save {
            head.content_length().ok()?
        } else {
            0
        };
        let head_end = head.header_end;
        if head_end + body_len > buf.len() {
            return None;
        }
        while len < head_end + body_len {
            let read = socket.read(&mut buf[len..]).await.ok()?;
            if read == 0 {
                return None;
            }
            len += read;
        }
        return Some((head_end, body_len, is_save));
    }
}

/// The new settings from the form. An empty password keeps the saved one
/// when the network name did not change.
fn settings_from_form(body: &[u8], current: &Settings) -> Result<Settings, &'static str> {
    const BAD_INPUT: &str = "A field is too long or not readable. Check it and try again.";
    let ssid = form::field(body, "ssid").ok_or(BAD_INPUT)?;
    let password = form::field(body, "password").ok_or(BAD_INPUT)?;
    let server = form::field(body, "server").ok_or(BAD_INPUT)?;
    let password = if password.is_empty() && ssid.as_str() == current.wifi_ssid.as_str() {
        current.wifi_password.as_str()
    } else {
        password.as_str()
    };
    Settings::new(ssid.as_str(), password, server.as_str()).map_err(|invalid| invalid.message())
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx = [0u8; 2 * dhcp::PACKET_MAX];
    let mut tx = [0u8; 2 * dhcp::PACKET_MAX];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    if let Err(err) = socket.bind(dhcp::SERVER_PORT) {
        log::error!("setup: dhcp bind failed: {err:?}");
        return;
    }
    let mut server = dhcp::DhcpServer::new(SETUP_IP);
    let mut request = [0u8; dhcp::PACKET_MAX];
    let mut reply = [0u8; dhcp::PACKET_MAX];
    loop {
        let Ok((len, _)) = socket.recv_from(&mut request).await else {
            continue;
        };
        if let Some(reply_len) = server.handle(&request[..len], &mut reply) {
            let to = (Ipv4Address::BROADCAST, dhcp::CLIENT_PORT);
            if let Err(err) = socket.send_to(&reply[..reply_len], to).await {
                log::warn!("setup: dhcp reply failed: {err:?}");
            }
        }
    }
}

#[embassy_executor::task]
async fn dns_task(stack: Stack<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx = [0u8; 1024];
    let mut tx = [0u8; 1024];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    if let Err(err) = socket.bind(dns::PORT) {
        log::error!("setup: dns bind failed: {err:?}");
        return;
    }
    let mut query = [0u8; dns::PACKET_MAX];
    let mut answer = [0u8; dns::PACKET_MAX];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut query).await else {
            continue;
        };
        if let Some(answer_len) = dns::answer(&query[..len], SETUP_IP, &mut answer) {
            let _ = socket.send_to(&answer[..answer_len], from).await;
        }
    }
}

/// Reboots into setup mode when `button` (active low, like BOOT on GPIO0)
/// is held for 3 seconds.
#[embassy_executor::task]
pub async fn button_task(button: Input<'static>) {
    loop {
        if button.is_low() {
            let pressed = Instant::now();
            while button.is_low() {
                if pressed.elapsed() >= BUTTON_HOLD {
                    log::info!("setup: button held, restarting into setup mode");
                    restart::reboot(NoteKind::Setup, "button held");
                }
                Timer::after(Duration::from_millis(50)).await;
            }
        }
        Timer::after(Duration::from_millis(100)).await;
    }
}

/// Reboots into setup mode when the station has had no network for
/// [`WIFI_DOWN_TIMEOUT`], so a board that moved, or whose router changed,
/// can be set up again without a cable.
#[embassy_executor::task]
pub async fn fallback_task(stack: Stack<'static>) {
    let mut last_up = Instant::now();
    loop {
        if stack.is_config_up() {
            last_up = Instant::now();
        } else if last_up.elapsed() >= WIFI_DOWN_TIMEOUT {
            log::warn!("setup: no Wi-Fi for 5 minutes, restarting into setup mode");
            restart::reboot(NoteKind::Setup, "no Wi-Fi for 5 minutes");
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}
