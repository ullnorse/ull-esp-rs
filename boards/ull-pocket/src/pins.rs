use esp_hal::peripherals as p;

pub(crate) struct EpdPins {
    pub(crate) spi: p::SPI2<'static>,
    pub(crate) busy: p::GPIO18<'static>,
    pub(crate) rst: p::GPIO19<'static>,
    pub(crate) dc: p::GPIO20<'static>,
    pub(crate) cs: p::GPIO21<'static>,
    pub(crate) sck: p::GPIO22<'static>,
    pub(crate) mosi: p::GPIO23<'static>,
}

pub(crate) struct BoardPins {
    pub(crate) bat_alert: Option<p::GPIO0<'static>>,
    pub(crate) usb_pgood: Option<p::GPIO1<'static>>,
    pub(crate) btn_up: Option<p::GPIO2<'static>>,
    pub(crate) acc_int1: Option<p::GPIO3<'static>>,
    pub(crate) baro_int: Option<p::GPIO4<'static>>,
    pub(crate) btn_ok: Option<p::GPIO5<'static>>,
    pub(crate) led_red: Option<p::GPIO8<'static>>,
    pub(crate) btn_dn: Option<p::GPIO9<'static>>,
    pub(crate) led_blue: Option<p::GPIO14<'static>>,
    pub(crate) chg_dis: Option<p::GPIO15<'static>>,
    pub(crate) epd: Option<EpdPins>,
}
